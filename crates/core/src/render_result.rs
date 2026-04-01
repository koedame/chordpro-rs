//! Structured render result type for capturing warnings during rendering.

/// Result of a render operation, containing both the rendered output
/// and any warnings produced during rendering.
///
/// Renderers collect warnings (e.g., transpose saturation, chorus recall
/// limits) into [`warnings`](Self::warnings) instead of printing them
/// directly. Callers can inspect and display warnings as they see fit.
#[derive(Debug, Clone)]
#[must_use]
pub struct RenderResult<T> {
    /// The rendered output.
    pub output: T,
    /// Warnings emitted during rendering.
    pub warnings: Vec<String>,
}

impl<T> RenderResult<T> {
    /// Create a new `RenderResult` with the given output and no warnings.
    pub fn new(output: T) -> Self {
        Self {
            output,
            warnings: Vec::new(),
        }
    }

    /// Create a new `RenderResult` with the given output and warnings.
    pub fn with_warnings(output: T, warnings: Vec<String>) -> Self {
        Self { output, warnings }
    }

    /// Returns `true` if there are no warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_no_warnings() {
        let result = RenderResult::new("hello");
        assert_eq!(result.output, "hello");
        assert!(result.warnings.is_empty());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_with_warnings() {
        let result = RenderResult::with_warnings("output", vec!["warning 1".to_string()]);
        assert_eq!(result.output, "output");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.has_warnings());
    }

    #[test]
    fn test_with_empty_warnings() {
        let result = RenderResult::with_warnings(42, Vec::new());
        assert_eq!(result.output, 42);
        assert!(!result.has_warnings());
    }
}
