use melsec_mc_mock::MockServer;

#[tokio::test]
async fn legacy_combined_key_roundtrip() {
    let s = MockServer::new();
    // set using combined key
    s.set_words("D100", 0, &[0x42u16]).await;
    let got = s.get_words("D", 100, 1).await;
    assert_eq!(got, vec![0x42u16]);
}

#[tokio::test]
async fn separated_key_roundtrip() {
    let s = MockServer::new();
    s.set_words("D", 200, &[0x99u16]).await;
    let got = s.get_words("D200", 0, 1).await;
    assert_eq!(got, vec![0x99u16]);
}

#[tokio::test]
async fn ambiguous_combined_key_prefers_addr() {
    let s = MockServer::new();
    // ambiguous: combined key present but explicit addr non-zero -> prefer explicit addr
    s.set_words("D300", 50, &[0x77u16]).await;
    // should be written at address 50
    let got = s.get_words("D", 50, 1).await;
    assert_eq!(got, vec![0x77u16]);
}
