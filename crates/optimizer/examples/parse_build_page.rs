//! Run a provider parser over a saved page and print what it recovered.
//!
//! Fixtures prove the shape; only a real page proves the site. Save one with
//! `fetch_build_page` (which uses the scraper's own headers), then:
//!
//!   cargo run -p gw2-optimizer --example parse_build_page -- guildjen page.html

use gw2_optimizer::providers;

fn main() {
    let mut args = std::env::args().skip(1);
    let site = args.next().unwrap_or_else(|| "guildjen".to_string());
    let Some(path) = args.next() else {
        eprintln!("usage: parse_build_page <guildjen|snowcrows|hardstuck> <file.html>");
        std::process::exit(2);
    };
    let html = std::fs::read_to_string(&path).expect("read the saved page");
    println!("{site}  {path}  ({} bytes)", html.len());

    let build = match site.as_str() {
        "guildjen" => providers::guildjen::parse(&html),
        "snowcrows" => providers::snowcrows::parse(&html),
        "hardstuck" => {
            let name = providers::hardstuck::build_name(&html).unwrap_or_default();
            println!("  name       : {name:?}");
            println!("  mode/scale : {:?}", providers::hardstuck::mode_and_scale(&html));
            println!("  class says : {:?}", providers::hardstuck::game_mode(&html));
            println!("  role       : {:?}", providers::role_in_name(&name));
            providers::hardstuck::parse(&html)
        }
        other => {
            eprintln!("no parser for {other} yet");
            std::process::exit(2);
        }
    };

    println!("  build code : {:?}", build.build_code);
    if let Some(code) = &build.build_code {
        match gw2_optimizer::build_template::decode(code) {
            Some(t) => println!(
                "    decodes to : profession {} specs {:?} skills {:?}",
                t.profession,
                t.specs.map(|s| s.id),
                t.skills
            ),
            None => println!("    DOES NOT DECODE"),
        }
    }
    println!("  rune       : {:?}", build.rune_id);
    println!("  sigils     : {:?}", build.sigil_ids);
    println!("  relic      : {:?}", build.relic_id);
    println!("  amulet     : {:?}", build.amulet_id);
    println!("  dominant stat: {:?}", build.dominant_stat());
    println!("  specs:");
    for spec in &build.specs {
        println!("    {:>3} -> {:?}", spec.id, spec.trait_ids);
    }
    println!("  gear ({} rows):", build.gear.len());
    for row in &build.gear {
        println!(
            "    {:<12} {:<14} item {:?} upgrades {:?}",
            row.slot, row.stat, row.item_id, row.upgrade_ids
        );
    }
}
