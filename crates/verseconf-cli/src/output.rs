use verseconf_core::ErrorReport;

#[allow(dead_code)]
pub fn format_error(report: &ErrorReport) -> String {
    report.format()
}
