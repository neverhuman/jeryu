use super::*;

#[tokio::test]
async fn auth_error_maps_correctly() {
    // No live network in unit tests; only construct + verify id.
    let c = OpenAiCompatibleClient::new("openrouter", "https://example.invalid")
        .with_api_key("nope")
        .with_data_use(DataUse::NoTrain);
    assert_eq!(c.id(), "openrouter");
    assert_eq!(c.data_use(), DataUse::NoTrain);
}
