use super::*;

fn probability(value: f64) -> Probability {
    Probability::try_from(value).expect("fixture probability is valid")
}

#[test]
fn value_objects_expose_their_typed_values() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let id = CandidateId::new(7);
    let candidate = Candidate::new(id, "banana");
    let judgment = Judgment::new(id, probability(0.75));

    assert_eq!(query.as_str(), "select fruit");
    assert_eq!(id.value(), 7);
    assert_eq!(candidate.text, "banana");
    assert_eq!(judgment.id, id);
    assert_eq!(judgment.probability.get(), 0.75);
}

#[test]
fn probability_accepts_boundaries_and_string_input() {
    assert_eq!(
        Probability::try_from(0.0).expect("zero is valid").get(),
        0.0
    );
    assert_eq!(Probability::try_from(1.0).expect("one is valid").get(), 1.0);
    assert_eq!(
        "0.25"
            .parse::<Probability>()
            .expect("number is valid")
            .get(),
        0.25
    );
}

#[test]
fn probability_rejects_bad_numeric_and_text_input() {
    for value in [f64::NAN, f64::NEG_INFINITY, -0.1, 1.1] {
        assert!(Probability::try_from(value).is_err());
    }
    assert!("not-a-number".parse::<Probability>().is_err());
}

#[test]
fn batch_plan_accepts_empty_and_complete_ranges() {
    assert!(BatchPlan::new(Vec::new()).validate(0).is_ok());
    assert!(BatchPlan::new(vec![0..2, 2..4]).validate(4).is_ok());
}

#[test]
fn batch_plan_rejects_empty_input_with_ranges() {
    assert!(
        BatchPlan::new(std::iter::once(0..1).collect())
            .validate(0)
            .is_err()
    );
}

#[test]
fn batch_plan_rejects_non_contiguous_empty_out_of_bounds_and_incomplete_ranges() {
    for plan in [
        BatchPlan::new(std::iter::once(1..2).collect()),
        BatchPlan::new(std::iter::once(0..0).collect()),
        BatchPlan::new(std::iter::once(0..3).collect()),
        BatchPlan::new(std::iter::once(0..1).collect()),
    ] {
        assert!(plan.validate(2).is_err());
    }
}

#[test]
fn batch_response_sorts_valid_judgments_and_rejects_invalid_ids() {
    let expected = [CandidateId::new(0), CandidateId::new(1)];
    let response = BatchResponse::try_new(
        &expected,
        vec![
            Judgment::new(expected[1], probability(0.8)),
            Judgment::new(expected[0], probability(0.2)),
        ],
    )
    .expect("response should be valid");
    assert_eq!(response.judgments[0].id, expected[0]);

    assert!(BatchResponse::try_new(&[expected[0], expected[0]], Vec::new()).is_err());
    assert!(BatchResponse::try_new(&expected, Vec::new()).is_err());
    assert!(
        BatchResponse::try_new(
            &expected,
            vec![
                Judgment::new(expected[0], probability(0.2)),
                Judgment::new(CandidateId::new(9), probability(0.8)),
            ],
        )
        .is_err()
    );
    assert!(
        BatchResponse::try_new(
            &expected,
            vec![
                Judgment::new(expected[0], probability(0.2)),
                Judgment::new(expected[0], probability(0.8)),
            ],
        )
        .is_err()
    );
}
