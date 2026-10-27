use pdfa_office::{engine::validate_report, model::PdfAStandard};

fn valid_report() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<report>
  <buildInformation><releaseDetails id="core" version="1.30.3"/></buildInformation>
  <jobs><job>
    <item size="128"><name>converted.pdf</name></item>
    <validationReport profileName="PDF/A-2b validation profile" isCompliant="true">
      <details passedRules="100" failedRules="0" passedChecks="200" failedChecks="0"/>
    </validationReport>
  </job></jobs>
  <batchSummary totalJobs="1" failedToParse="0" encrypted="0" outOfMemory="0" veraExceptions="0">
    <validationReports compliant="1" nonCompliant="0" failedJobs="0">1</validationReports>
  </batchSummary>
</report>"#
        .into()
}

#[test]
fn accepts_complete_matching_report() {
    let summary = validate_report(&valid_report(), PdfAStandard::A2b).unwrap();
    assert_eq!(summary.verapdf_version, "1.30.3");
}

#[test]
fn rejects_wrong_profile_and_noncompliance() {
    assert!(validate_report(&valid_report(), PdfAStandard::A3b).is_err());
    assert!(validate_report(
        &valid_report().replace("isCompliant=\"true\"", "isCompliant=\"false\""),
        PdfAStandard::A2b
    )
    .is_err());
}

#[test]
fn rejects_missing_truncated_and_duplicate_jobs() {
    assert!(validate_report("<report/>", PdfAStandard::A2b).is_err());
    assert!(validate_report(
        "<report><validationReport isCompliant=\"true\">",
        PdfAStandard::A2b
    )
    .is_err());
    let multiple = valid_report().replace(
        "</jobs>",
        "<job><item><name>second.pdf</name></item></job></jobs>",
    );
    assert!(validate_report(&multiple, PdfAStandard::A2b).is_err());
    let multiple_reports =
        valid_report().replace("</job>", "<validationReport isCompliant=\"true\"/></job>");
    assert!(validate_report(&multiple_reports, PdfAStandard::A2b).is_err());
}

#[test]
fn rejects_exceptions_and_inconsistent_counters() {
    let exception = valid_report().replace(
        "</job>",
        "<taskException>Failed to parse</taskException></job>",
    );
    assert!(validate_report(&exception, PdfAStandard::A2b).is_err());
    for (from, to) in [
        ("failedToParse=\"0\"", "failedToParse=\"1\""),
        ("outOfMemory=\"0\"", "outOfMemory=\"1\""),
        ("totalJobs=\"1\"", "totalJobs=\"0\""),
        ("failedRules=\"0\"", "failedRules=\"1\""),
        ("passedChecks=\"200\"", "passedChecks=\"0\""),
        ("failedJobs=\"0\"", "failedJobs=\"1\""),
    ] {
        assert!(
            validate_report(&valid_report().replace(from, to), PdfAStandard::A2b).is_err(),
            "{from}"
        );
    }
}

#[test]
fn rejects_xml_external_entities() {
    let xml = "<!DOCTYPE report [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><report>&x;</report>";
    assert!(validate_report(xml, PdfAStandard::A2b).is_err());
}
