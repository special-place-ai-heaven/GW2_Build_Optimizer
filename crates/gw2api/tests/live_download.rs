//! Live integration test for GW2 API download.
//! Run with: cargo test -p gw2-api --test live_download -- --ignored --nocapture

use std::time::Instant;

#[test]
#[ignore] // Requires network
fn test_full_download_pipeline() {
    let client = gw2_api::client::Gw2Client::without_key().unwrap();

    // Build number
    let build = client.get_build_number().unwrap();
    println!("[OK] Build: {}", build);

    // Traits
    let start = Instant::now();
    let traits: Vec<gw2_api::models::Trait> = client.fetch_all("traits").unwrap();
    println!(
        "[OK] Traits: {} in {:.1}s",
        traits.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(
        traits.len() > 100,
        "Expected >100 traits, got {}",
        traits.len()
    );

    // Skills
    let start = Instant::now();
    let skills: Vec<gw2_api::models::Skill> = client.fetch_all("skills").unwrap();
    println!(
        "[OK] Skills: {} in {:.1}s",
        skills.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(
        skills.len() > 100,
        "Expected >100 skills, got {}",
        skills.len()
    );

    // Specializations
    let start = Instant::now();
    let specs: Vec<gw2_api::models::Specialization> = client.fetch_all("specializations").unwrap();
    println!(
        "[OK] Specs: {} in {:.1}s",
        specs.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(specs.len() > 30, "Expected >30 specs, got {}", specs.len());

    // Itemstats
    let start = Instant::now();
    let itemstats: Vec<gw2_api::models::ItemStat> = client.fetch_all("itemstats").unwrap();
    println!(
        "[OK] Itemstats: {} in {:.1}s",
        itemstats.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(
        itemstats.len() > 50,
        "Expected >50 itemstats, got {}",
        itemstats.len()
    );

    // Items (first 2000 only — full download too slow for test)
    let start = Instant::now();
    let all_ids: Vec<serde_json::Value> = client.get("items").unwrap();
    println!(
        "[OK] Item IDs: {} in {:.1}s",
        all_ids.len(),
        start.elapsed().as_secs_f64()
    );

    let subset = &all_ids[..2000.min(all_ids.len())];
    let start = Instant::now();
    let items: Vec<serde_json::Value> = client.fetch_by_ids("items", subset).unwrap();
    println!(
        "[OK] Items (2000 subset): {} in {:.1}s",
        items.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(
        items.len() > 1000,
        "Expected >1000 items from 2000 IDs, got {}",
        items.len()
    );

    // Professions
    let start = Instant::now();
    let profs: Vec<gw2_api::models::Profession> = client
        .get_with_params(
            "professions",
            &[("ids", "all"), ("v", "2019-12-19T00:00:00.000Z")],
        )
        .unwrap();
    println!(
        "[OK] Professions: {} in {:.1}s",
        profs.len(),
        start.elapsed().as_secs_f64()
    );
    assert_eq!(
        profs.len(),
        9,
        "Expected 9 professions, got {}",
        profs.len()
    );

    println!("\n=== ALL ENDPOINTS OK ===");
}
