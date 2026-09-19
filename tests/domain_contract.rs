use nlgrep::{BatchPlan, BatchResponse, CandidateId, Judgment, Probability, Query};

fn probability(value: f64) -> Probability {
    Probability::try_from(value).expect("fixture probability is valid")
}

#[test]
fn probability_rejects_non_finite_and_out_of_range_values() {
    assert!(Probability::try_from(f64::NAN).is_err());
    assert!(Probability::try_from(-0.01).is_err());
    assert!(Probability::try_from(1.01).is_err());
    assert_eq!(
        Probability::try_from(0.5).expect("valid probability").get(),
        0.5
    );
}

#[test]
fn query_rejects_empty_and_whitespace_values() {
    assert!(Query::try_new("").is_err());
    assert!(Query::try_new(" \t\n").is_err());
    assert_eq!(
        Query::try_new("select fruit")
            .expect("valid query")
            .as_str(),
        "select fruit"
    );
}

#[test]
fn batch_response_rejects_missing_duplicate_and_unknown_ids() {
    let expected = [CandidateId::new(0), CandidateId::new(1)];
    let missing = vec![Judgment::new(expected[0], probability(0.8))];
    let duplicate = vec![
        Judgment::new(expected[0], probability(0.8)),
        Judgment::new(expected[0], probability(0.7)),
    ];
    let unknown = vec![
        Judgment::new(expected[0], probability(0.8)),
        Judgment::new(CandidateId::new(9), probability(0.7)),
    ];

    assert!(BatchResponse::try_new(&expected, missing).is_err());
    assert!(BatchResponse::try_new(&expected, duplicate).is_err());
    assert!(BatchResponse::try_new(&expected, unknown).is_err());
}

#[test]
fn batch_plan_must_cover_input_without_overlap() {
    let plan = BatchPlan::new(vec![0..2, 2..4]);
    plan.validate(4).expect("contiguous plan is valid");
    assert!(BatchPlan::new(vec![0..3, 2..4]).validate(4).is_err());
}
