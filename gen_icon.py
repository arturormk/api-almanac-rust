#!/usr/bin/env python3
"""Generate the API Almanac app icon as an SVG."""
import math

W, H = 1024, 1024
CX = W // 2  # 512

# --- A letterform geometry ---
APEX_X, APEX_Y = CX, 248           # top of the A
FOOT_L_X, FOOT_R_X = 205, 819      # outer foot x positions
FOOT_Y = 776                        # baseline y
LEG_T = 98                          # leg stroke thickness (perpendicular)
HT = LEG_T / 2

# Left leg vector (apex → left foot)
lv = (FOOT_L_X - APEX_X, FOOT_Y - APEX_Y)
llen = math.hypot(*lv)
lu = (lv[0]/llen, lv[1]/llen)      # unit along left leg
lp_out = (-lu[1], lu[0])           # perpendicular left (outward)
lp_in  = ( lu[1], -lu[0])          # perpendicular right (inward)

# Right leg (symmetric)
rv = (FOOT_R_X - APEX_X, FOOT_Y - APEX_Y)
rlen = math.hypot(*rv)
ru = (rv[0]/rlen, rv[1]/rlen)
rp_out = ( ru[1], -ru[0])          # outward right
rp_in  = (-ru[1],  ru[0])          # inward left

def offset(pt, direction, d):
    return (pt[0] + direction[0]*d, pt[1] + direction[1]*d)

apex = (APEX_X, APEX_Y)
foot_l = (FOOT_L_X, FOOT_Y)
foot_r = (FOOT_R_X, FOOT_Y)

# Four corners of each leg (outer-top, outer-bottom, inner-bottom, inner-top)
lo_top = offset(apex,   lp_out, HT)
lo_bot = offset(foot_l, lp_out, HT)
li_top = offset(apex,   lp_in,  HT)
li_bot = offset(foot_l, lp_in,  HT)

ro_top = offset(apex,   rp_out, HT)
ro_bot = offset(foot_r, rp_out, HT)
ri_top = offset(apex,   rp_in,  HT)
ri_bot = offset(foot_r, rp_in,  HT)

# Clip all bottom points to FOOT_Y (flat baseline cap)
def clip_y(pt, y_cap, direction_y_sign):
    """If the point is past the cap, project the edge line to y_cap."""
    return pt  # Already computed at foot; no need to clip since we use foot_l/r

# Crossbar: at vertical fraction of A height
CB_FRAC = 0.49    # crossbar center at 49% of A height (from apex)
CB_HALF_H = 44    # half-height of crossbar

cb_y_top = APEX_Y + (FOOT_Y - APEX_Y) * CB_FRAC - CB_HALF_H
cb_y_bot = APEX_Y + (FOOT_Y - APEX_Y) * CB_FRAC + CB_HALF_H

# x of left inner edge at crossbar y levels
def left_inner_x(y):
    # Line through li_top and li_bot
    t = (y - li_top[1]) / (li_bot[1] - li_top[1])
    return li_top[0] + t * (li_bot[0] - li_top[0])

def right_inner_x(y):
    t = (y - ri_top[1]) / (ri_bot[1] - ri_top[1])
    return ri_top[0] + t * (ri_bot[0] - ri_top[0])

cb_l_top = (left_inner_x(cb_y_top), cb_y_top)
cb_l_bot = (left_inner_x(cb_y_bot), cb_y_bot)
cb_r_top = (right_inner_x(cb_y_top), cb_y_top)
cb_r_bot = (right_inner_x(cb_y_bot), cb_y_bot)

# The full A outline (clockwise, single polygon, no inner hole needed):
# Outer left → apex → outer right → down right outer → right foot →
# right inner bottom → up inner right to crossbar bottom →
# crossbar across to left → down inner left → left inner bottom →
# back left outer bottom.
#
# Inner triangle (above crossbar) is part of the filled polygon,
# so the "A" reads from the two-leg silhouette + crossbar gap at feet.

def pt(p): return f"{p[0]:.1f},{p[1]:.1f}"

# Apex outer edges meet — average the two top points to get a single apex
apex_lo = lo_top  # left outer top
apex_ro = ro_top  # right outer top

# Full A polygon (with the inner triangle INCLUDED in fill — bold solid A)
# The shape: outer left bottom → outer left up to apex → outer right down →
# outer right bottom → inner right bottom → inner right up to crossbar →
# crossbar left → inner left down to inner left bottom → close
a_poly = [
    lo_bot,    # outer left bottom (left foot outer)
    lo_top,    # outer left top (near apex)
    apex,      # apex point (center top)
    ro_top,    # outer right top (near apex)
    ro_bot,    # outer right bottom (right foot outer)
    ri_bot,    # inner right bottom (right foot inner)
    cb_r_bot,  # inner right at crossbar bottom
    cb_l_bot,  # inner left at crossbar bottom  (crossbar bottom edge)
    li_bot,    # inner left bottom (left foot inner)
]

# This draws a solid A shape with the inner triangle filled.
# We must ALSO exclude the inner triangle above crossbar by adding the
# crossbar top edge and going up the inner right to apex, then back down.
# Let's use evenodd fill with an inner cutout triangle.

# Outer A polygon (full outer silhouette):
outer = [
    lo_bot, lo_top, apex, ro_top, ro_bot, ri_bot, cb_r_bot, cb_l_bot, li_bot
]

# Inner triangle cutout (above crossbar):
cb_r_top_pt = cb_r_top
cb_l_top_pt = cb_l_top
li_top_adj = (left_inner_x(li_top[1]), li_top[1])  # = li_top

inner_triangle = [
    li_top,   # inner left top (near apex, inner)
    ri_top,   # inner right top (near apex, inner)
    cb_r_top, # inner right at crossbar top
    cb_l_top, # inner left at crossbar top
]

def poly_to_path(pts, close=True):
    d = "M " + pt(pts[0])
    for p in pts[1:]:
        d += " L " + pt(p)
    if close:
        d += " Z"
    return d

outer_path = poly_to_path(outer)
inner_path = poly_to_path(inner_triangle)

# Combined path (evenodd: outer filled, inner hole)
combined = outer_path + " " + inner_path

# --- Signal arcs (Wi-Fi style, above apex) ---
ARC_CX = CX
ARC_CY = APEX_Y - 18    # just above the apex
ARC_RADII = [56, 96, 136]
ARC_STROKE = 30
ARC_GAP = 22             # gap between arcs and center dot

DOT_R = 20

def arc_path(cx, cy, r):
    # Upward semicircle: start at (cx-r, cy), sweep CW to (cx+r, cy)
    # This traces the upper half of the circle
    return f"M {cx-r:.1f},{cy:.1f} A {r},{r} 0 0 1 {cx+r:.1f},{cy:.1f}"

# --- SVG assembly ---
svg = f"""<?xml version="1.0" encoding="UTF-8"?>
<svg width="{W}" height="{H}" viewBox="0 0 {W} {H}"
     xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"
                    gradientUnits="objectBoundingBox">
      <stop offset="0%"   stop-color="#1a4a8a"/>
      <stop offset="100%" stop-color="#2e76cc"/>
    </linearGradient>
  </defs>

  <!-- Background -->
  <rect width="{W}" height="{H}" rx="185" ry="185" fill="url(#bg)"/>

  <!-- Letter A (evenodd: outer fill minus inner triangle) -->
  <path d="{combined}"
        fill="white" fill-rule="evenodd"/>

  <!-- Signal arcs -->
  <g fill="none" stroke="white" stroke-linecap="round">
"""

for r in ARC_RADII:
    svg += f'    <path d="{arc_path(ARC_CX, ARC_CY, r)}" stroke-width="{ARC_STROKE}"/>\n'

svg += f"""  </g>

  <!-- Center dot -->
  <circle cx="{ARC_CX}" cy="{ARC_CY}" r="{DOT_R}" fill="white"/>

</svg>
"""

out = "/home/arturo/work/github/apialmanac-rust/icon_source.svg"
with open(out, "w") as f:
    f.write(svg)

print("Written:", out)
print(f"A polygon vertices ({len(outer)} outer + {len(inner_triangle)} inner):")
for p in outer:
    print(f"  {p[0]:.1f}, {p[1]:.1f}")
print("Crossbar:", f"y={cb_y_top:.0f}–{cb_y_bot:.0f}, x={cb_l_top[0]:.0f}–{cb_r_top[0]:.0f}")
print("Arcs at y=", ARC_CY, "radii=", ARC_RADII)
