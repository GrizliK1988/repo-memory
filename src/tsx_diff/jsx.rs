use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::Node;

use crate::changes::{ChangeKind, ChangeValue, ChildChange};

pub(super) type Elements = BTreeMap<String, Element>;

pub(super) struct Element {
    component: String,
    props: BTreeMap<String, Value>,
}

struct Value {
    source: ChangeValue,
    structure: Option<(Shape, BTreeMap<String, Value>)>,
}

#[derive(PartialEq, Eq)]
enum Shape {
    Object,
    Array(usize),
    Guard(String),
}

pub(super) fn extract(node: Node<'_>, source: &str) -> Elements {
    let mut elements = BTreeMap::new();
    visit(node, source, "", &mut BTreeMap::new(), &mut elements);
    elements
}

fn visit(
    node: Node<'_>,
    source: &str,
    parent_path: &str,
    occurrences: &mut BTreeMap<String, usize>,
    elements: &mut Elements,
) {
    let opening = match node.kind() {
        "jsx_element" => node.child_by_field_name("open_tag"),
        "jsx_self_closing_element" => Some(node),
        _ => None,
    };
    let mut path = parent_path.to_owned();
    if let Some(opening) = opening {
        let component = opening
            .child_by_field_name("name")
            .map(|n| &source[n.byte_range()])
            .unwrap_or("<fragment>");
        let prefix = if parent_path.is_empty() {
            component.to_owned()
        } else {
            format!("{parent_path}/{component}")
        };
        let occurrence = occurrences.entry(prefix.clone()).or_default();
        path = format!("{prefix}[{occurrence}]");
        *occurrence += 1;
        let mut props = BTreeMap::new();
        let mut cursor = opening.walk();
        for attribute in opening
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "jsx_attribute")
        {
            let mut cursor = attribute.walk();
            let mut children = attribute
                .named_children(&mut cursor)
                .filter(|n| n.kind() != "comment");
            if let Some(name) = children.next() {
                let value = children
                    .next()
                    .map(|n| value(n, source))
                    .unwrap_or_else(|| Value {
                        source: ChangeValue {
                            text: "true".into(),
                            ..location(attribute, source)
                        },
                        structure: None,
                    });
                props.insert(source[name.byte_range()].to_owned(), value);
            }
        }
        elements.insert(
            path.clone(),
            Element {
                component: component.into(),
                props,
            },
        );
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, source, &path, occurrences, elements);
    }
}

fn location(node: Node<'_>, source: &str) -> ChangeValue {
    ChangeValue {
        text: source[node.byte_range()].to_owned(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + usize::from(node.end_position().column > 0),
    }
}

fn value(mut node: Node<'_>, source: &str) -> Value {
    // JSX braces and grouping parentheses are syntax wrappers, not property paths.
    while matches!(node.kind(), "jsx_expression" | "parenthesized_expression") {
        let mut cursor = node.walk();
        let Some(child) = node
            .named_children(&mut cursor)
            .find(|n| n.kind() != "comment")
        else {
            break;
        };
        node = child;
    }
    Value {
        source: location(node, source),
        structure: structure(node, source),
    }
}

fn structure(node: Node<'_>, source: &str) -> Option<(Shape, BTreeMap<String, Value>)> {
    let mut children = BTreeMap::new();
    let shape = match node.kind() {
        "object" => {
            let mut cursor = node.walk();
            for property in node
                .named_children(&mut cursor)
                .filter(|n| n.kind() != "comment")
            {
                let (key, property_value) = match property.kind() {
                    "pair" => {
                        let key = property.child_by_field_name("key")?;
                        let path = match key.kind() {
                            "property_identifier" => format!(".{}", &source[key.byte_range()]),
                            "string" | "number" => format!("[{}]", &source[key.byte_range()]),
                            _ => return None,
                        };
                        (path, value(property.child_by_field_name("value")?, source))
                    }
                    "shorthand_property_identifier" => (
                        format!(".{}", &source[property.byte_range()]),
                        value(property, source),
                    ),
                    // Spreads, dynamic keys and methods require expression-level reporting.
                    _ => return None,
                };
                if children.insert(key, property_value).is_some() {
                    return None; // Duplicate keys have order-dependent semantics.
                }
            }
            Shape::Object
        }
        "array" => {
            let mut cursor = node.walk();
            let mut expecting_value = true;
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "[" | "]" | "comment" => {}
                    "," if expecting_value => return None, // Sparse arrays: keep the full expression.
                    "," => expecting_value = true,
                    "spread_element" => return None,
                    _ if child.is_named() => {
                        children.insert(format!("[{}]", children.len()), value(child, source));
                        expecting_value = false;
                    }
                    _ => return None,
                }
            }
            Shape::Array(children.len())
        }
        "binary_expression" if node.child_by_field_name("operator")?.kind() == "&&" => {
            let guard = node.child_by_field_name("left")?;
            children.insert(
                String::new(),
                value(node.child_by_field_name("right")?, source),
            );
            Shape::Guard(source[guard.byte_range()].to_owned())
        }
        _ => return None,
    };
    Some((shape, children))
}

pub(super) fn compare(before: &Elements, after: &Elements) -> Vec<ChildChange> {
    let mut changes = Vec::new();
    for element_path in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        let old = before.get(element_path);
        let new = after.get(element_path);
        let component = &new
            .or(old)
            .expect("element exists in at least one snapshot")
            .component;
        let old_props = old.map(|e| &e.props);
        let new_props = new.map(|e| &e.props);
        let keys: BTreeSet<_> = old_props
            .into_iter()
            .flat_map(|p| p.keys())
            .chain(new_props.into_iter().flat_map(|p| p.keys()))
            .collect();
        for prop in keys {
            let context = ChildChange {
                kind: ChangeKind::Changed,
                target: component.clone(),
                target_path: element_path.clone(),
                input: prop.clone(),
                path: prop.clone(),
                before: None,
                after: None,
            };
            compare_value(
                old_props.and_then(|p| p.get(prop)),
                new_props.and_then(|p| p.get(prop)),
                context,
                &mut changes,
            );
        }
    }
    changes
}

fn compare_value(
    old: Option<&Value>,
    new: Option<&Value>,
    mut change: ChildChange,
    changes: &mut Vec<ChildChange>,
) {
    if let (Some(old), Some(new)) = (old, new) {
        if old.source.text == new.source.text {
            return;
        }
        if let (Some((old_shape, old_children)), Some((new_shape, new_children))) =
            (&old.structure, &new.structure)
            && old_shape == new_shape
        {
            for key in old_children
                .keys()
                .chain(new_children.keys())
                .collect::<BTreeSet<_>>()
            {
                let mut nested = change.clone();
                nested.path.push_str(key);
                compare_value(
                    old_children.get(key),
                    new_children.get(key),
                    nested,
                    changes,
                );
            }
            return;
        }
    }
    change.kind = match (old, new) {
        (None, Some(_)) => ChangeKind::Added,
        (Some(_), None) => ChangeKind::Removed,
        _ => ChangeKind::Changed,
    };
    change.before = old.map(|v| v.source.clone());
    change.after = new.map(|v| v.source.clone());
    changes.push(change);
}
