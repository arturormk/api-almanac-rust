use serde_json::Value;
use std::collections::HashMap;

// ── SketchNode ─────────────────────────────────────────────────────────────

/// The inferred type of a JSON value — not a formal schema, just an observed shape.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchNode {
    Str,
    Email,
    Url,
    Datetime,
    Date,
    Uuid,
    Integer,
    Float,
    Boolean,
    /// A field observed as null; treated as "probably nullable of some type".
    Nullable(Box<SketchNode>),
    Object(Vec<(String, SketchNode)>),
    /// Homogeneous array; `Empty` item means the array was empty.
    Array(Box<SketchNode>),
    Empty,
    Mixed,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Infer a `SketchNode` tree from a JSON value.
pub fn sketch_json(value: &Value) -> SketchNode {
    sketch_value(value)
}

/// Render a `SketchNode` as a YAML string.
pub fn to_yaml_string(node: &SketchNode) -> String {
    let yaml_val = to_yaml_value(node);
    serde_yaml::to_string(&yaml_val).unwrap_or_default()
}

/// Infer a `SketchNode` tree from an XML string.
///
/// The root element becomes the single top-level key. Repeated same-name
/// sibling elements are treated as arrays. Attributes are prefixed with `@`.
/// Text content of mixed elements (attributes + children + text) is keyed as `_`.
pub fn sketch_xml(xml_str: &str) -> Result<SketchNode, String> {
    let doc = roxmltree::Document::parse(xml_str).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let name = root.tag_name().name().to_string();
    let inner = sketch_element(root);
    Ok(SketchNode::Object(vec![(name, inner)]))
}

// ── XML inference ──────────────────────────────────────────────────────────

fn sketch_element(node: roxmltree::Node) -> SketchNode {
    let attrs: Vec<(String, SketchNode)> = node
        .attributes()
        .map(|a| (format!("@{}", a.name()), classify_string(a.value())))
        .collect();

    // Group child elements by local name, preserving insertion order.
    let mut child_order: Vec<String> = Vec::new();
    let mut child_groups: HashMap<String, Vec<SketchNode>> = HashMap::new();
    for child in node.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name().to_string();
        let entry = child_groups.entry(tag.clone()).or_insert_with(|| {
            child_order.push(tag);
            Vec::new()
        });
        entry.push(sketch_element(child));
    }

    let text = node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .map(str::trim)
        .find(|t| !t.is_empty());

    // Pure leaf: no attributes, no child elements.
    if attrs.is_empty() && child_groups.is_empty() {
        return match text {
            Some(t) => classify_string(t),
            None => SketchNode::Str,
        };
    }

    let mut fields: Vec<(String, SketchNode)> = attrs;

    for tag in &child_order {
        let group = &child_groups[tag];
        let node_sketch = if group.len() > 1 {
            SketchNode::Array(Box::new(merge_types(group)))
        } else {
            group[0].clone()
        };
        fields.push((tag.clone(), node_sketch));
    }

    // Mixed content: element has both child elements (or attributes) and text.
    if let Some(t) = text {
        if !child_groups.is_empty() {
            fields.push(("_".to_string(), classify_string(t)));
        }
    }

    SketchNode::Object(fields)
}

// ── Inference ──────────────────────────────────────────────────────────────

fn sketch_value(value: &Value) -> SketchNode {
    match value {
        Value::Null => SketchNode::Nullable(Box::new(SketchNode::Str)),
        Value::Bool(_) => SketchNode::Boolean,
        Value::Number(n) => {
            if n.is_f64() && !n.is_i64() && !n.is_u64() {
                SketchNode::Float
            } else {
                SketchNode::Integer
            }
        }
        Value::String(s) => classify_string(s),
        Value::Array(items) => {
            if items.is_empty() {
                SketchNode::Array(Box::new(SketchNode::Empty))
            } else {
                let types: Vec<SketchNode> = items.iter().map(sketch_value).collect();
                SketchNode::Array(Box::new(merge_types(&types)))
            }
        }
        Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), sketch_value(v)))
                .collect();
            SketchNode::Object(fields)
        }
    }
}

fn classify_string(s: &str) -> SketchNode {
    if is_uuid(s) {
        return SketchNode::Uuid;
    }
    if is_datetime(s) {
        return SketchNode::Datetime;
    }
    if is_date(s) {
        return SketchNode::Date;
    }
    if is_url(s) {
        return SketchNode::Url;
    }
    if is_email(s) {
        return SketchNode::Email;
    }
    SketchNode::Str
}

// ── String pattern detection ───────────────────────────────────────────────

fn is_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected = [8usize, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected.iter())
        .all(|(p, &len)| p.len() == len && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_date_prefix(b: &[u8]) -> bool {
    b.len() >= 10
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'-'
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn is_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 16 && is_date_prefix(b) && b[10] == b'T'
}

fn is_date(s: &str) -> bool {
    s.len() == 10 && is_date_prefix(s.as_bytes())
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_email(s: &str) -> bool {
    match s.find('@') {
        Some(at) if at > 0 => {
            let domain = &s[at + 1..];
            !domain.is_empty() && domain.contains('.') && !domain.starts_with('.')
        }
        _ => false,
    }
}

// ── Array element type merging ─────────────────────────────────────────────

fn merge_types(types: &[SketchNode]) -> SketchNode {
    if types.is_empty() {
        return SketchNode::Empty;
    }

    // Homogeneous arrays of objects → merge schemas field-by-field
    if types.iter().all(|t| matches!(t, SketchNode::Object(_))) {
        return merge_objects(types, types.len());
    }

    // Arrays of objects → merge inner schemas (e.g. same repeated child across different parents)
    if types.iter().all(|t| matches!(t, SketchNode::Array(i) if matches!(i.as_ref(), SketchNode::Object(_)))) {
        let inners: Vec<SketchNode> = types.iter().filter_map(|t| {
            if let SketchNode::Array(inner) = t { Some((**inner).clone()) } else { None }
        }).collect();
        return SketchNode::Array(Box::new(merge_objects(&inners, inners.len())));
    }

    // All same → return that type
    if types.iter().all(|t| t == &types[0]) {
        return types[0].clone();
    }

    // Mix of T and Nullable(T) → Nullable(T)
    let non_null: Vec<&SketchNode> = types
        .iter()
        .filter(|t| !matches!(t, SketchNode::Nullable(_)))
        .collect();
    let has_nullable = types.iter().any(|t| matches!(t, SketchNode::Nullable(_)));
    if has_nullable && !non_null.is_empty() {
        let base = non_null[0];
        if non_null.iter().all(|t| *t == base) {
            return SketchNode::Nullable(Box::new((*base).clone()));
        }
    }

    SketchNode::Mixed
}

/// Merge an array of Object nodes into a single Object whose field types are
/// the union of all observed values. Fields absent in some objects become nullable.
fn merge_objects(objects: &[SketchNode], total: usize) -> SketchNode {
    let mut all_keys: Vec<String> = Vec::new();
    for obj in objects {
        if let SketchNode::Object(fields) = obj {
            for (k, _) in fields {
                if !all_keys.contains(k) {
                    all_keys.push(k.clone());
                }
            }
        }
    }

    let fields: Vec<(String, SketchNode)> = all_keys
        .into_iter()
        .map(|key| {
            let vals: Vec<SketchNode> = objects
                .iter()
                .filter_map(|obj| {
                    if let SketchNode::Object(fields) = obj {
                        fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let merged = if vals.len() < total {
                let inner = merge_types(&vals);
                match inner {
                    SketchNode::Nullable(_) => inner,
                    other => SketchNode::Nullable(Box::new(other)),
                }
            } else {
                merge_types(&vals)
            };
            (key, merged)
        })
        .collect();

    SketchNode::Object(fields)
}

// ── YAML rendering ─────────────────────────────────────────────────────────

fn to_yaml_value(node: &SketchNode) -> serde_yaml::Value {
    match node {
        SketchNode::Object(fields) => {
            let mut map = serde_yaml::Mapping::new();
            for (k, v) in fields {
                map.insert(serde_yaml::Value::String(k.clone()), to_yaml_value(v));
            }
            serde_yaml::Value::Mapping(map)
        }
        SketchNode::Array(item) => {
            serde_yaml::Value::Sequence(vec![to_yaml_value(item)])
        }
        SketchNode::Empty => serde_yaml::Value::Sequence(vec![]),
        SketchNode::Nullable(inner) => match inner.as_ref() {
            SketchNode::Object(_) | SketchNode::Array(_) => to_yaml_value(inner),
            other => serde_yaml::Value::String(format!("{} | null", type_label(other))),
        },
        scalar => serde_yaml::Value::String(type_label(scalar).to_string()),
    }
}

fn type_label(node: &SketchNode) -> &'static str {
    match node {
        SketchNode::Str => "string",
        SketchNode::Email => "email",
        SketchNode::Url => "url",
        SketchNode::Datetime => "datetime",
        SketchNode::Date => "date",
        SketchNode::Uuid => "uuid",
        SketchNode::Integer => "integer",
        SketchNode::Float => "float",
        SketchNode::Boolean => "boolean",
        SketchNode::Empty => "[]",
        SketchNode::Mixed => "mixed",
        _ => "unknown",
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(json: &str) -> String {
        let v: Value = serde_json::from_str(json).unwrap();
        to_yaml_string(&sketch_json(&v))
    }

    #[test]
    fn scalar_types() {
        let j = r#"{
            "id":         "usr_abc123",
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "email":      "ada@example.com",
            "profile_url":"https://example.com/ada",
            "created_at": "2026-05-20T13:12:30Z",
            "birth_date": "1990-01-15",
            "name":       "Ada Lovelace",
            "age":        36,
            "score":      9.5,
            "active":     true,
            "deleted_at": null
        }"#;
        let out = yaml(j);
        assert!(out.contains("session_id: uuid"),    "uuid: {out}");
        assert!(out.contains("email: email"),         "email: {out}");
        assert!(out.contains("profile_url: url"),     "url: {out}");
        assert!(out.contains("created_at: datetime"), "datetime: {out}");
        assert!(out.contains("birth_date: date"),     "date: {out}");
        assert!(out.contains("name: string"),         "string: {out}");
        assert!(out.contains("age: integer"),         "integer: {out}");
        assert!(out.contains("score: float"),         "float: {out}");
        assert!(out.contains("active: boolean"),      "boolean: {out}");
        assert!(out.contains("null"),                 "nullable: {out}");
    }

    #[test]
    fn nested_object() {
        let out = yaml(r#"{"user":{"id":"u1","email":"a@b.com"}}"#);
        assert!(out.contains("user:"), "{out}");
        assert!(out.contains("email: email"), "{out}");
    }

    #[test]
    fn array_of_primitives() {
        let out = yaml(r#"{"roles":["admin","user"]}"#);
        assert!(out.contains("roles:"), "{out}");
        assert!(out.contains("- string"), "{out}");
    }

    #[test]
    fn array_of_objects() {
        let out = yaml(r#"[{"id":1,"name":"Ada"},{"id":2,"name":"Grace"}]"#);
        assert!(out.contains("- id: integer"), "{out}");
        assert!(out.contains("name: string"), "{out}");
    }

    #[test]
    fn empty_array() {
        let out = yaml(r#"{"tags":[]}"#);
        assert!(out.contains("tags:"), "{out}");
    }

    #[test]
    fn uuid_detection() {
        assert_eq!(classify_string("550e8400-e29b-41d4-a716-446655440000"), SketchNode::Uuid);
        assert_eq!(classify_string("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"), SketchNode::Uuid);
        assert_eq!(classify_string("not-a-uuid"), SketchNode::Str);
    }

    #[test]
    fn datetime_vs_date() {
        assert_eq!(classify_string("2026-05-20T13:12:30Z"), SketchNode::Datetime);
        assert_eq!(classify_string("2026-05-20"), SketchNode::Date);
        assert_eq!(classify_string("2026-05-20 extra"), SketchNode::Str);
    }

    #[test]
    fn array_object_merge_absent_field_becomes_nullable() {
        let out = yaml(r#"[{"id":1,"name":"Ada"},{"id":2}]"#);
        assert!(out.contains("null"), "absent field should be nullable: {out}");
    }

    fn xml_sketch(xml: &str) -> String {
        to_yaml_string(&sketch_xml(xml).expect("valid XML"))
    }

    #[test]
    fn xml_simple_leaf() {
        let out = xml_sketch("<status>active</status>");
        assert!(out.contains("status: string"), "{out}");
    }

    #[test]
    fn xml_element_with_attributes() {
        let out = xml_sketch(r#"<user id="550e8400-e29b-41d4-a716-446655440000" email="ada@example.com"/>"#);
        assert!(out.contains("'@id': uuid"), "{out}");
        assert!(out.contains("'@email': email"), "{out}");
    }

    #[test]
    fn xml_repeated_elements() {
        let out = xml_sketch(r#"
            <users>
                <user><name>Ada</name><email>ada@example.com</email></user>
                <user><name>Grace</name><email>grace@example.com</email></user>
            </users>
        "#);
        assert!(out.contains("user:"), "{out}");
        assert!(out.contains("- name: string"), "{out}");
        assert!(out.contains("email: email"), "{out}");
    }

    #[test]
    fn xml_attributes_and_text() {
        let out = xml_sketch(r#"<price currency="USD">9.99</price>"#);
        assert!(out.contains("'@currency': string"), "{out}");
        // pure leaf with attribute — no "_" key since there are no child elements
        assert!(out.contains("price:"), "{out}");
    }

    #[test]
    fn xml_nullable_object_renders_deep() {
        // One recinto has evento children, another doesn't → evento should show
        // full sub-structure (not "unknown | null") because Nullable(Array(Object))
        // must render structurally.
        let out = xml_sketch(r#"
            <cartelera>
                <recinto id="1">
                    <evento><nombre>Concierto</nombre><fecha>2026-06-10</fecha></evento>
                    <evento><nombre>Teatro</nombre><fecha>2026-06-11</fecha></evento>
                </recinto>
                <recinto id="2"/>
            </cartelera>
        "#);
        assert!(!out.contains("unknown"), "should not contain 'unknown': {out}");
        assert!(out.contains("nombre"), "should drill into evento fields: {out}");
        assert!(out.contains("fecha"), "should show fecha field: {out}");
    }

    #[test]
    fn xml_array_merged_across_parents() {
        // Two recintos each have repeated evento children but with different fields.
        // Should produce a merged schema, not "mixed".
        let out = xml_sketch(r#"
            <cartelera>
                <recinto id="1">
                    <evento><nombre>Concierto</nombre></evento>
                    <evento><nombre>Teatro</nombre></evento>
                </recinto>
                <recinto id="2">
                    <evento><nombre>Opera</nombre><url>https://example.com</url></evento>
                    <evento><nombre>Ballet</nombre><url>https://example.com/2</url></evento>
                </recinto>
            </cartelera>
        "#);
        assert!(!out.contains("mixed"), "should not be mixed: {out}");
        assert!(out.contains("nombre"), "should show nombre field: {out}");
    }

    #[test]
    fn xml_nested() {
        let out = xml_sketch(r#"
            <response>
                <meta>
                    <created_at>2026-05-20T13:12:30Z</created_at>
                    <total>42</total>
                </meta>
                <items>
                    <item id="1"><url>https://example.com/1</url></item>
                    <item id="2"><url>https://example.com/2</url></item>
                </items>
            </response>
        "#);
        assert!(out.contains("response:"), "{out}");
        assert!(out.contains("created_at: datetime"), "{out}");
        assert!(out.contains("'@id': string"), "{out}");
        assert!(out.contains("url: url"), "{out}");
    }
}
