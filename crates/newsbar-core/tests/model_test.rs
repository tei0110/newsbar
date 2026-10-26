use newsbar_core::model::SummaryStatus;

#[test]
fn summary_status_round_trips_through_str() {
    for s in [
        SummaryStatus::Pending,
        SummaryStatus::Done,
        SummaryStatus::Skipped,
        SummaryStatus::Failed,
    ] {
        assert_eq!(SummaryStatus::parse(s.as_str()), Some(s));
    }
    assert_eq!(SummaryStatus::Pending.as_str(), "pending");
    assert_eq!(SummaryStatus::parse("unknown"), None);
}
