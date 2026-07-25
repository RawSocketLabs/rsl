pub fn render_label(label: &Option<String>) -> String {
    match label.as_deref() {
        Some(label) => format!("[{label}]"),
        None => "[untitled]".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::render_label;

    #[test]
    fn renders_present_and_absent_labels() {
        assert_eq!(render_label(&Some("station".to_owned())), "[station]");
        assert_eq!(render_label(&None), "[untitled]");
    }
}
