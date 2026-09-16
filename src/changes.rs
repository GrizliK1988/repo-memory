//! Language-independent descriptions of changes within an enclosing symbol.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

/// Source expression and one-based inclusive line range in its snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeValue {
    /// Expression text; language adapters may normalize implicit values.
    pub text: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// A change to an input or nested value on a child construct within a symbol.
///
/// Language adapters determine which constructs they support and how to identify
/// them. For example, a target can be a JSX component or a called function, and
/// an input can name a property or identify an argument position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildChange {
    pub kind: ChangeKind,
    /// Name of the child construct, e.g. `View` or a function name.
    pub target: String,
    /// Adapter-defined occurrence path relative to the enclosing symbol.
    pub target_path: String,
    /// Name or positional identifier of the affected input, e.g. `style`.
    pub input: String,
    /// Input plus nested property/index path, e.g. `style[2].flexShrink`.
    pub path: String,
    pub before: Option<ChangeValue>,
    pub after: Option<ChangeValue>,
}
