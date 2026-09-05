# Audit remediation ledger

Source: docs/audit/AUDIT-REPORT-2026-09-05.md (2026-09-05). Baseline: main at 2d66490 / 1.11.27. Original source line numbers are audit anchors, not current edit coordinates.

267 W findings + B001. No remedy is marked complete merely by being planned. R001–R003 remain excluded per the report. Original severity is preserved; execution order is risk-adjusted in tasks.md. For every row below: use SymForge get_file_context/get_symbol/find_references before edits, edit_plan before symbol edits, analyze_file_impact after file changes. Code deletion requires fresh caller and compatibility analysis.

## Closure rules

Implemented means code changed; verified means relevant checks passed; accepted-in-game is a separate batch gate. Duplicate entries close only with their canonical remedy. Retained-deliberate entries need current evidence and a reconsideration trigger. Contested entries remain open until rechecked. Never change a calibrated rule or erase historical data merely to close debt.

## Priority index

| Order | ID | Audit severity/verdict | Story | State | Source |
|---|---|---|---|---|---|
| 1 | [W011](#w011) | S2/confirmed | US1 | verified-scoped | `crates/core/src/feedback/store.rs:50` |
| 2 | [W006](#w006) | S2/confirmed | US1 | verified-scoped | `crates/addon/src/ui/main_view/lock_panel.rs:686` |
| 3 | [W002](#w002) | S2/confirmed | US1 | verified-scoped | `crates/addon/src/radio/player.rs:1077` |
| 4 | [W012](#w012) | S2/confirmed | US1 | verified-scoped | `crates/core/src/feedback/store.rs:113` |
| 5 | [W001](#w001) | S2/confirmed | US1 | verified-scoped | `.github/workflows/ci.yml:17` |
| 6 | [B001](#b001) | S2/observed | US2 | verified-scoped | `crates/addon/src/ui/main_view/optimization.rs:1050` |
| 7 | [W025](#w025) | S2/confirmed | US2 | verified-scoped | `crates/optimizer/src/gemini_tools.rs:1529` |
| 8 | [W035](#w035) | S2/confirmed | US2 | planned | `crates/optimizer/src/rotation/simulator.rs:220` |
| 9 | [W019](#w019) | S2/confirmed | US2 | planned | `crates/optimizer/src/data/objective_profiles.rs:86` |
| 10 | [W044](#w044) | S2/confirmed | US2 | planned | `data/normalized_effects/2026-01-13/pve.json:2` |
| 11 | [W013](#w013) | S2/confirmed | US2 | planned | `crates/optimizer/src/balance.rs:11` |
| 12 | [W015](#w015) | S2/confirmed | US2 | planned | `crates/optimizer/src/data/manifests.rs:43` |
| 13 | [W016](#w016) | S2/confirmed | US2 | planned | `crates/optimizer/src/data/mod.rs:137` |
| 14 | [W039](#w039) | S2/confirmed | US2 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:1389` |
| 15 | [W038](#w038) | S2/confirmed | US2 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:861` |
| 16 | [W008](#w008) | S2/confirmed | US2 | planned | `crates/addon/src/ui/main_view/optimization.rs:714` |
| 17 | [W004](#w004) | S2/confirmed | US2 | planned | `crates/addon/src/state.rs:898` |
| 18 | [W009](#w009) | S2/confirmed | US2 | planned | `crates/addon/src/ui/main_view/tabs/settings.rs:60` |
| 19 | [W082](#w082) | S3/confirmed | US2 | planned | `crates/addon/src/ui/main_view/tabs/saveload.rs:509` |
| 20 | [W182](#w182) | S3/confirmed | US2 | planned | `crates/optimizer/src/synergy_pipeline.rs:859` |
| 21 | [W225](#w225) | S4/confirmed | US2 | planned | `crates/optimizer/src/llm/openai.rs:325` |
| 22 | [W157](#w157) | S3/confirmed | US2 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:1109` |
| 23 | [W231](#w231) | S4/confirmed | US2 | planned | `crates/optimizer/src/llm/sse.rs:317` |
| 24 | [W034](#w034) | S2/confirmed | US2 | planned | `crates/optimizer/src/referee.rs:937` |
| 25 | [W003](#w003) | S2/confirmed | US3 | planned | `crates/addon/src/radio/player.rs:1357` |
| 26 | [W005](#w005) | S2/confirmed | US3 | planned | `crates/addon/src/ui/gear_sheet.rs:250` |
| 27 | [W007](#w007) | S2/confirmed | US3 | planned | `crates/addon/src/ui/main_view/lock_panel.rs:790` |
| 28 | [W010](#w010) | S2/confirmed | US3 | planned | `crates/addon/src/ui/mod.rs:212` |
| 29 | [W014](#w014) | S2/confirmed | US3 | planned | `crates/optimizer/src/data/balance_overrides.rs:302` |
| 30 | [W017](#w017) | S2/confirmed | US3 | planned | `crates/optimizer/src/data/normalized_effects.rs:522` |
| 31 | [W018](#w018) | S2/confirmed | US3 | planned | `crates/optimizer/src/data/normalized_effects.rs:724` |
| 32 | [W020](#w020) | S2/confirmed | US3 | planned | `crates/optimizer/src/data/patch_ledger.rs:17` |
| 33 | [W021](#w021) | S2/confirmed | US3 | planned | `crates/optimizer/src/data/rotation_profiles.rs:119` |
| 34 | [W022](#w022) | S2/confirmed | US3 | planned | `crates/optimizer/src/engine.rs:70` |
| 35 | [W023](#w023) | S2/confirmed | US3 | planned | `crates/optimizer/src/engine.rs:1103` |
| 36 | [W024](#w024) | S2/confirmed | US3 | planned | `crates/optimizer/src/gamedb.rs:655` |
| 37 | [W026](#w026) | S2/confirmed | US3 | planned | `crates/optimizer/src/gemini_tools.rs:1913` |
| 38 | [W027](#w027) | S2/confirmed | US3 | planned | `crates/optimizer/src/llm/anthropic.rs:373` |
| 39 | [W028](#w028) | S2/confirmed | US3 | planned | `crates/optimizer/src/llm/mod.rs:159` |
| 40 | [W029](#w029) | S2/confirmed | US3 | planned | `crates/optimizer/src/llm/mod.rs:189` |
| 41 | [W030](#w030) | S2/confirmed | US3 | planned | `crates/optimizer/src/llm/openai.rs:267` |
| 42 | [W031](#w031) | S2/confirmed | US3 | planned | `crates/optimizer/src/llm/openai.rs:267` |
| 43 | [W032](#w032) | S2/confirmed | US3 | planned | `crates/optimizer/src/parser_consistency_tests.rs:3` |
| 44 | [W033](#w033) | S2/confirmed | US3 | planned | `crates/optimizer/src/prompts.rs:401` |
| 45 | [W036](#w036) | S2/confirmed | US3 | planned | `crates/optimizer/src/rotation/skill_timings.rs:18` |
| 46 | [W037](#w037) | S2/confirmed | US3 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:357` |
| 47 | [W040](#w040) | S2/confirmed | US3 | planned | `crates/optimizer/src/scoring.rs:400` |
| 48 | [W041](#w041) | S2/confirmed | US3 | planned | `crates/optimizer/src/scraper.rs:952` |
| 49 | [W042](#w042) | S2/confirmed | US3 | planned | `crates/optimizer/src/synergy.rs:111` |
| 50 | [W043](#w043) | S2/confirmed | US3 | planned | `crates/optimizer/src/text_util.rs:14` |
| 51 | [W045](#w045) | S2/confirmed | US3 | planned | `docs/architecture.md:21` |
| 52 | [W046](#w046) | S3/confirmed | US4 | planned | `.gitignore:32` |
| 53 | [W047](#w047) | S3/confirmed | US4 | planned | `crates/addon/src/lib.rs:53` |
| 54 | [W048](#w048) | S3/confirmed | US4 | planned | `crates/addon/src/news.rs:183` |
| 55 | [W049](#w049) | S3/confirmed | US4 | planned | `crates/addon/src/news_art.rs:394` |
| 56 | [W050](#w050) | S3/confirmed | US4 | planned | `crates/addon/src/radio/art.rs:6` |
| 57 | [W051](#w051) | S3/confirmed | US4 | planned | `crates/addon/src/radio/art.rs:492` |
| 58 | [W052](#w052) | S3/confirmed | US4 | planned | `crates/addon/src/radio/player.rs:1069` |
| 59 | [W053](#w053) | S3/confirmed | US4 | planned | `crates/addon/src/state.rs:530` |
| 60 | [W054](#w054) | S3/confirmed | US4 | planned | `crates/addon/src/state.rs:830` |
| 61 | [W055](#w055) | S3/confirmed | US4 | planned | `crates/addon/src/ui/comparison.rs:557` |
| 62 | [W056](#w056) | S3/confirmed | US4 | planned | `crates/addon/src/ui/comparison.rs:1171` |
| 63 | [W057](#w057) | S3/confirmed | US4 | planned | `crates/addon/src/ui/gear_diff.rs:20` |
| 64 | [W058](#w058) | S3/confirmed | US4 | planned | `crates/addon/src/ui/gear_sheet.rs:344` |
| 65 | [W059](#w059) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/character.rs:37` |
| 66 | [W060](#w060) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/lock_panel.rs:79` |
| 67 | [W061](#w061) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/lock_panel.rs:869` |
| 68 | [W062](#w062) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/mod.rs:76` |
| 69 | [W063](#w063) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/mod.rs:288` |
| 70 | [W064](#w064) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/mod.rs:942` |
| 71 | [W065](#w065) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimization.rs:108` |
| 72 | [W066](#w066) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimization.rs:212` |
| 73 | [W067](#w067) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimization.rs:491` |
| 74 | [W068](#w068) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimization.rs:626` |
| 75 | [W069](#w069) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimization.rs:1585` |
| 76 | [W070](#w070) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/optimize_flow.rs:229` |
| 77 | [W071](#w071) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/resolution.rs:308` |
| 78 | [W072](#w072) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/resolution.rs:420` |
| 79 | [W073](#w073) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/about.rs:182` |
| 80 | [W074](#w074) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/about.rs:433` |
| 81 | [W075](#w075) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/about.rs:472` |
| 82 | [W076](#w076) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/about/wizard.rs:579` |
| 83 | [W077](#w077) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/improve.rs:84` |
| 84 | [W078](#w078) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/news.rs:37` |
| 85 | [W079](#w079) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/radio.rs:443` |
| 86 | [W080](#w080) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/saveload.rs:69` |
| 87 | [W081](#w081) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/saveload.rs:193` |
| 88 | [W083](#w083) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/saveload.rs:832` |
| 89 | [W084](#w084) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/settings.rs:300` |
| 90 | [W085](#w085) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/settings.rs:520` |
| 91 | [W086](#w086) | S3/confirmed | US4 | planned | `crates/addon/src/ui/main_view/tabs/settings.rs:595` |
| 92 | [W087](#w087) | S3/confirmed | US4 | planned | `crates/addon/src/ui/news_feed.rs:406` |
| 93 | [W088](#w088) | S3/confirmed | US4 | planned | `crates/addon/src/ui/radar_chart.rs:286` |
| 94 | [W089](#w089) | S3/confirmed | US4 | planned | `crates/addon/src/ui/setup.rs:556` |
| 95 | [W090](#w090) | S3/confirmed | US4 | planned | `crates/addon/src/ui/theme.rs:544` |
| 96 | [W091](#w091) | S3/confirmed | US4 | planned | `crates/addon/src/ui/theme.rs:924` |
| 97 | [W092](#w092) | S3/confirmed | US4 | planned | `crates/core/src/feedback/report.rs:10` |
| 98 | [W093](#w093) | S3/confirmed | US4 | planned | `crates/core/src/storage.rs:135` |
| 99 | [W094](#w094) | S3/confirmed | US4 | planned | `crates/core/src/storage.rs:184` |
| 100 | [W095](#w095) | S3/confirmed | US4 | planned | `crates/core/src/types.rs:99` |
| 101 | [W096](#w096) | S3/confirmed | US4 | planned | `crates/gw2api/src/cache.rs:44` |
| 102 | [W097](#w097) | S3/confirmed | US4 | planned | `crates/gw2api/src/cache.rs:118` |
| 103 | [W098](#w098) | S3/confirmed | US4 | planned | `crates/gw2api/src/client.rs:490` |
| 104 | [W099](#w099) | S3/confirmed | US4 | planned | `crates/gw2api/src/client.rs:767` |
| 105 | [W100](#w100) | S3/confirmed | US4 | planned | `crates/gw2api/src/download.rs:46` |
| 106 | [W101](#w101) | S3/confirmed | US4 | planned | `crates/gw2api/src/localize.rs:111` |
| 107 | [W102](#w102) | S3/confirmed | US4 | planned | `crates/optimizer/src/combat.rs:226` |
| 108 | [W103](#w103) | S3/confirmed | US4 | planned | `crates/optimizer/src/combat.rs:769` |
| 109 | [W104](#w104) | S3/confirmed | US4 | planned | `crates/optimizer/src/combat.rs:1324` |
| 110 | [W105](#w105) | S3/confirmed | US4 | planned | `crates/optimizer/src/context.rs:416` |
| 111 | [W106](#w106) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/balance_overrides.rs:327` |
| 112 | [W107](#w107) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/boon_condition_formulas.rs:227` |
| 113 | [W108](#w108) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/mod.rs:69` |
| 114 | [W109](#w109) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/normalized_effects.rs:649` |
| 115 | [W110](#w110) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/objective_profiles.rs:120` |
| 116 | [W111](#w111) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/quality.rs:80` |
| 117 | [W112](#w112) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/rotation_profiles.rs:267` |
| 118 | [W113](#w113) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/rotation_profiles.rs:408` |
| 119 | [W114](#w114) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/rotation_profiles.rs:416` |
| 120 | [W115](#w115) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/slot_budgets.rs:209` |
| 121 | [W116](#w116) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/slot_budgets.rs:225` |
| 122 | [W117](#w117) | S3/confirmed | US4 | planned | `crates/optimizer/src/data/universal_formulas.rs:100` |
| 123 | [W118](#w118) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:434` |
| 124 | [W119](#w119) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:1273` |
| 125 | [W120](#w120) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:1292` |
| 126 | [W121](#w121) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:1957` |
| 127 | [W122](#w122) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:2285` |
| 128 | [W123](#w123) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:2410` |
| 129 | [W124](#w124) | S3/confirmed | US4 | planned | `crates/optimizer/src/engine.rs:2469` |
| 130 | [W125](#w125) | S3/confirmed | US4 | planned | `crates/optimizer/src/gamedb.rs:742` |
| 131 | [W126](#w126) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini.rs:825` |
| 132 | [W127](#w127) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini.rs:970` |
| 133 | [W128](#w128) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini.rs:1041` |
| 134 | [W129](#w129) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini_tools.rs:3` |
| 135 | [W130](#w130) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini_tools.rs:27` |
| 136 | [W131](#w131) | S3/confirmed | US4 | planned | `crates/optimizer/src/gemini_tools.rs:943` |
| 137 | [W132](#w132) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/anthropic.rs:110` |
| 138 | [W133](#w133) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/anthropic.rs:110` |
| 139 | [W134](#w134) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/anthropic.rs:340` |
| 140 | [W135](#w135) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/anthropic.rs:559` |
| 141 | [W136](#w136) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/body.rs:17` |
| 142 | [W137](#w137) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/gemini.rs:19` |
| 143 | [W138](#w138) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/gemini.rs:19` |
| 144 | [W139](#w139) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/gemini.rs:71` |
| 145 | [W140](#w140) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/gemini.rs:71` |
| 146 | [W141](#w141) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/mod.rs:153` |
| 147 | [W142](#w142) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/mod.rs:189` |
| 148 | [W143](#w143) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/mod.rs:195` |
| 149 | [W144](#w144) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/openai.rs:54` |
| 150 | [W145](#w145) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/openai.rs:150` |
| 151 | [W146](#w146) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/openai.rs:150` |
| 152 | [W147](#w147) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/openai.rs:418` |
| 153 | [W148](#w148) | S3/confirmed | US4 | planned | `crates/optimizer/src/llm/openai.rs:418` |
| 154 | [W149](#w149) | S3/confirmed | US4 | planned | `crates/optimizer/src/referee.rs:49` |
| 155 | [W150](#w150) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/builder.rs:20` |
| 156 | [W151](#w151) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/builder.rs:656` |
| 157 | [W152](#w152) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/mod.rs:196` |
| 158 | [W153](#w153) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/simulator.rs:643` |
| 159 | [W154](#w154) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:59` |
| 160 | [W155](#w155) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:428` |
| 161 | [W156](#w156) | S3/confirmed | US4 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:963` |
| 162 | [W158](#w158) | S3/confirmed | US4 | planned | `crates/optimizer/src/scenario.rs:13` |
| 163 | [W159](#w159) | S3/confirmed | US4 | planned | `crates/optimizer/src/scenario.rs:113` |
| 164 | [W160](#w160) | S3/confirmed | US4 | planned | `crates/optimizer/src/scenario.rs:220` |
| 165 | [W161](#w161) | S3/confirmed | US4 | planned | `crates/optimizer/src/scoring.rs:275` |
| 166 | [W162](#w162) | S3/confirmed | US4 | planned | `crates/optimizer/src/scraper.rs:27` |
| 167 | [W163](#w163) | S3/confirmed | US4 | planned | `crates/optimizer/src/scraper.rs:84` |
| 168 | [W164](#w164) | S3/confirmed | US4 | planned | `crates/optimizer/src/scraper.rs:276` |
| 169 | [W165](#w165) | S3/confirmed | US4 | planned | `crates/optimizer/src/scraper.rs:1118` |
| 170 | [W166](#w166) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:91` |
| 171 | [W167](#w167) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:336` |
| 172 | [W168](#w168) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:772` |
| 173 | [W169](#w169) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:1451` |
| 174 | [W170](#w170) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:1469` |
| 175 | [W171](#w171) | S3/confirmed | US4 | planned | `crates/optimizer/src/search_v2.rs:1998` |
| 176 | [W172](#w172) | S3/confirmed | US4 | planned | `crates/optimizer/src/sigil_slots.rs:51` |
| 177 | [W173](#w173) | S3/confirmed | US4 | planned | `crates/optimizer/src/stats.rs:147` |
| 178 | [W174](#w174) | S3/confirmed | US4 | planned | `crates/optimizer/src/stats.rs:348` |
| 179 | [W175](#w175) | S3/confirmed | US4 | planned | `crates/optimizer/src/stats.rs:431` |
| 180 | [W176](#w176) | S3/confirmed | US4 | planned | `crates/optimizer/src/stats.rs:601` |
| 181 | [W177](#w177) | S3/confirmed | US4 | planned | `crates/optimizer/src/synergy.rs:149` |
| 182 | [W178](#w178) | S3/confirmed | US4 | planned | `crates/optimizer/src/synergy.rs:265` |
| 183 | [W179](#w179) | S3/confirmed | US4 | planned | `crates/optimizer/src/synergy_pipeline.rs:46` |
| 184 | [W180](#w180) | S3/confirmed | US4 | planned | `crates/optimizer/src/synergy_pipeline.rs:86` |
| 185 | [W181](#w181) | S3/confirmed | US4 | planned | `crates/optimizer/src/synergy_pipeline.rs:744` |
| 186 | [W183](#w183) | S3/confirmed | US4 | planned | `crates/optimizer/src/validation.rs:308` |
| 187 | [W184](#w184) | S3/confirmed | US4 | planned | `crates/optimizer/tests/objective_profiles_integration.rs:476` |
| 188 | [W185](#w185) | S3/confirmed | US4 | planned | `docs/superpowers/plans/2026-08-24-feedback-server.md:1290` |
| 189 | [W186](#w186) | S3/confirmed | US4 | planned | `docs/superpowers/plans/2026-08-26-per-slot-gear-implementation.md:141` |
| 190 | [W187](#w187) | S3/confirmed | US4 | planned | `docs/superpowers/specs/2026-08-24-feedback-and-about-design.md:360` |
| 191 | [W188](#w188) | S3/confirmed | US4 | planned | `docs/superpowers/specs/2026-08-26-per-slot-gear-design.md:77` |
| 192 | [W189](#w189) | S3/confirmed | US4 | planned | `locales/en.json:419` |
| 193 | [W190](#w190) | S3/confirmed | US4 | planned | `server/feedback/src/admin.html:56` |
| 194 | [W191](#w191) | S3/confirmed | US4 | planned | `server/feedback/src/admin.html:83` |
| 195 | [W192](#w192) | S3/confirmed | US4 | planned | `server/feedback/src/admin.html:308` |
| 196 | [W193](#w193) | S3/confirmed | US4 | planned | `server/feedback/src/admin.rs:154` |
| 197 | [W194](#w194) | S3/confirmed | US4 | planned | `server/feedback/src/reports.rs:65` |
| 198 | [W195](#w195) | S4/confirmed | US5 | planned | `crates/addon/src/feedback/client.rs:200` |
| 199 | [W196](#w196) | S4/confirmed | US5 | planned | `crates/addon/src/news.rs:34` |
| 200 | [W197](#w197) | S4/confirmed | US5 | planned | `crates/addon/src/radio/logos.rs:322` |
| 201 | [W198](#w198) | S4/confirmed | US5 | planned | `crates/addon/src/state.rs:326` |
| 202 | [W199](#w199) | S4/confirmed | US5 | planned | `crates/addon/src/ui/chat_bar.rs:559` |
| 203 | [W200](#w200) | S4/confirmed | US5 | planned | `crates/addon/src/ui/comparison.rs:861` |
| 204 | [W201](#w201) | S4/confirmed | US5 | planned | `crates/addon/src/ui/fonts.rs:168` |
| 205 | [W202](#w202) | S4/confirmed | US5 | planned | `crates/addon/src/ui/fonts.rs:306` |
| 206 | [W203](#w203) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/optimize_flow.rs:66` |
| 207 | [W204](#w204) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/optimize_flow.rs:292` |
| 208 | [W205](#w205) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/resolution.rs:13` |
| 209 | [W206](#w206) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/tabs/mod.rs:3` |
| 210 | [W207](#w207) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/tabs/radio.rs:776` |
| 211 | [W208](#w208) | S4/confirmed | US5 | planned | `crates/addon/src/ui/main_view/tabs/settings.rs:70` |
| 212 | [W209](#w209) | S4/confirmed | US5 | planned | `crates/addon/src/ui/radar_chart.rs:68` |
| 213 | [W210](#w210) | S4/confirmed | US5 | planned | `crates/core/src/config.rs:586` |
| 214 | [W211](#w211) | S4/confirmed | US5 | planned | `crates/core/src/config.rs:841` |
| 215 | [W212](#w212) | S4/confirmed | US5 | planned | `crates/core/src/storage.rs:249` |
| 216 | [W213](#w213) | S4/confirmed | US5 | planned | `crates/gw2api/src/cache.rs:80` |
| 217 | [W214](#w214) | S4/confirmed | US5 | planned | `crates/gw2api/src/graphics.rs:166` |
| 218 | [W215](#w215) | S4/confirmed | US5 | planned | `crates/gw2api/src/localize.rs:195` |
| 219 | [W216](#w216) | S4/confirmed | US5 | planned | `crates/optimizer/examples/nudge_druid_check.rs:58` |
| 220 | [W217](#w217) | S4/confirmed | US5 | planned | `crates/optimizer/src/data/cleanse_sources.rs:92` |
| 221 | [W218](#w218) | S4/confirmed | US5 | planned | `crates/optimizer/src/data/slot_budgets.rs:155` |
| 222 | [W219](#w219) | S4/confirmed | US5 | planned | `crates/optimizer/src/gamedb.rs:687` |
| 223 | [W220](#w220) | S4/confirmed | US5 | planned | `crates/optimizer/src/gemini.rs:24` |
| 224 | [W221](#w221) | S4/confirmed | US5 | planned | `crates/optimizer/src/gemini.rs:266` |
| 225 | [W222](#w222) | S4/confirmed | US5 | planned | `crates/optimizer/src/gemini_tools.rs:93` |
| 226 | [W223](#w223) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/anthropic.rs:336` |
| 227 | [W224](#w224) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/body.rs:17` |
| 228 | [W226](#w226) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/openai_compat.rs:74` |
| 229 | [W227](#w227) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/rate.rs:173` |
| 230 | [W228](#w228) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/response_cache.rs:52` |
| 231 | [W229](#w229) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/response_cache.rs:52` |
| 232 | [W230](#w230) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/sse.rs:9` |
| 233 | [W232](#w232) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/tools.rs:2` |
| 234 | [W233](#w233) | S4/confirmed | US5 | planned | `crates/optimizer/src/llm/tools.rs:3` |
| 235 | [W234](#w234) | S4/confirmed | US5 | planned | `crates/optimizer/src/parser_consistency_tests.rs:25` |
| 236 | [W235](#w235) | S4/confirmed | US5 | planned | `crates/optimizer/src/prompts.rs:313` |
| 237 | [W236](#w236) | S4/confirmed | US5 | planned | `crates/optimizer/src/rotation/simulator.rs:1048` |
| 238 | [W237](#w237) | S4/confirmed | US5 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:3` |
| 239 | [W238](#w238) | S4/confirmed | US5 | planned | `crates/optimizer/src/rotation/wvw_timeline.rs:417` |
| 240 | [W239](#w239) | S4/confirmed | US5 | planned | `crates/optimizer/src/scoring.rs:77` |
| 241 | [W240](#w240) | S4/confirmed | US5 | planned | `crates/optimizer/src/search.rs:47` |
| 242 | [W241](#w241) | S4/confirmed | US5 | planned | `crates/optimizer/src/search_v2.rs:726` |
| 243 | [W242](#w242) | S4/confirmed | US5 | planned | `crates/optimizer/src/search_v2.rs:1233` |
| 244 | [W243](#w243) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy.rs:31` |
| 245 | [W244](#w244) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy.rs:46` |
| 246 | [W245](#w245) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy.rs:708` |
| 247 | [W246](#w246) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy_pipeline.rs:377` |
| 248 | [W247](#w247) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy_pipeline.rs:486` |
| 249 | [W248](#w248) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy_pipeline.rs:1384` |
| 250 | [W249](#w249) | S4/confirmed | US5 | planned | `crates/optimizer/src/synergy_pipeline.rs:1409` |
| 251 | [W250](#w250) | S4/confirmed | US5 | planned | `crates/optimizer/src/upgrade_graph.rs:111` |
| 252 | [W251](#w251) | S4/confirmed | US5 | planned | `crates/optimizer/src/validation.rs:810` |
| 253 | [W252](#w252) | S4/confirmed | US5 | planned | `crates/optimizer/tests/live_llm.rs:200` |
| 254 | [W253](#w253) | S4/confirmed | US5 | planned | `crates/optimizer/tests/math_permutations.rs:2` |
| 255 | [W254](#w254) | S4/confirmed | US5 | planned | `crates/optimizer/tests/scoring_regression.rs:146` |
| 256 | [W255](#w255) | S4/confirmed | US5 | planned | `crates/optimizer/tests/upgrade_graph_live.rs:43` |
| 257 | [W256](#w256) | S4/confirmed | US5 | planned | `docs/architecture.md:7` |
| 258 | [W257](#w257) | S4/confirmed | US5 | planned | `docs/superpowers/plans/2026-08-26-per-slot-gear-implementation.md:155` |
| 259 | [W258](#w258) | S4/confirmed | US5 | planned | `server/feedback/Cargo.toml:23` |
| 260 | [W259](#w259) | S4/confirmed | US5 | planned | `server/feedback/src/ratelimit.rs:20` |
| 261 | [W260](#w260) | S4/confirmed | US5 | planned | `server/feedback/src/ratelimit.rs:109` |
| 262 | [W261](#w261) | S4/confirmed | US5 | planned | `server/feedback/tests/api.rs:813` |
| 263 | [W262](#w262) | S3/contested | US5 | planned | `crates/addon/src/ui/icons.rs:17` |
| 264 | [W263](#w263) | S3/contested | US5 | planned | `crates/gw2api/src/dev_config.rs:48` |
| 265 | [W264](#w264) | S4/contested | US5 | planned | `crates/addon/src/ui/main_view/tabs/radio.rs:1520` |
| 266 | [W265](#w265) | S4/contested | US5 | planned | `crates/optimizer/examples/nudge_druid_check.rs:140` |
| 267 | [W266](#w266) | S4/contested | US5 | planned | `crates/optimizer/src/llm/anthropic.rs:797` |
| 268 | [W267](#w267) | S4/contested | US5 | planned | `crates/optimizer/src/search_v2.rs:1221` |

## Finding details

### W011

- Task: T003 [US1]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `crates/core/src/feedback/store.rs:50`.
- Dependencies: story entry gate.

Claim: A `messages.json` that exists but cannot be read or parsed is silently downgraded to an empty message list, and the addon then writes that empty list back over the file, destroying the player's feedback history. `FeedbackStore::load` (store.rs:44-51) maps any read/parse error to `MessagesFile::default()` with only an `eprintln!` (invisible in an injected DLL). `crates/addon/src/feedback/tasks.rs:476-477` assigns that result straight into state (`feedback.messages = file.messages`), and `flush_dirty` (tasks.rs:688-700) later calls `FeedbackStore::save(&file)` with the in-memory snapshot, publishing the empty list. This is the exact failure mode `AppConfig::SavePolicy::RefuseUnreadFileOnDisk` (config.rs:66, config.rs:854-861) was added to prevent for `config.json` - a transient antivirus/cloud-sync sharing violation - but the feedback store has no equivalent guard.

Remediation decision: Return a fallible history load; keep a session write-refusal on addon feedback state after any non-NotFound failure, surface the error, and propagate refusal into flush_dirty. Never replace failed-read history with a default snapshot.

Verification: Current source path confirmed via SymForge; regression/implementation pending.

Acceptance: Demonstrate the invalid-input or failed-I/O path is safe, the normal path still works, and relevant tests plus strict Clippy pass.

### W006

- Task: T004 [US1]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `crates/addon/src/ui/main_view/lock_panel.rs:686`.
- Dependencies: story entry gate.

Claim: The "Lock All" button indexes the fixed-size array `locks.specs: [Option<u32>; 3]` (crates/core/src/types.rs:64) with `slot` from `current_specs.iter().enumerate()` (line 685), and `current_specs` has no length clamp anywhere on its path: tabs/improve.rs:62 maps it 1:1 from `ResolvedBuild::specializations`, which resolution.rs:281-311 builds by `filter_map` over the build tab deserialized from the GW2 API / the `char_*_buildtabs.json` disk cache. grep for `specs\[` / `take(3)` on that path shows no truncation; the rest of the same file iterates `for slot in 0..3` instead. A build tab with 4+ resolvable specs panics with index-out-of-bounds inside the ImGui render callback.

Remediation decision: Extract the Lock All spec/trait mutation into a helper that iterates only locks.specs.len() entries; preserve the existing bounded trait-column lookup. Exercise the helper with excess and malformed input.

Verification: Current source path confirmed via SymForge; regression/implementation pending.

Acceptance: Demonstrate the invalid-input or failed-I/O path is safe, the normal path still works, and relevant tests plus strict Clippy pass.

### W002

- Task: T005 [US1]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `crates/addon/src/radio/player.rs:1077`.
- Dependencies: story entry gate.

Claim: `stream_host_reserved` is a second, diverged copy of the reserved-address guard that `radio/logos.rs` implements as `url_ok` + `host_resolves_reserved`. The logos copy routes every host through `normalized_host` (logos.rs:83) whose doc states the reason verbatim: "`Url::host_str` keeps the brackets on IPv6 literals (`\"[::1]\"`), which would slip past the `IpAddr` parse". The player copy has no bracket stripping: it feeds the raw `host_str()` straight into `host.parse::<std::net::IpAddr>()` (line 1084) and, on failure, into `(host, port).to_socket_addrs()` (line 1088). For a station URL like `http://[::1]:8000/stream` the parse fails on the brackets, and the fallback resolve of the literal string "[::1]" is resolver-dependent, so the guard the comment at lines 974-977 describes ("The station URL is community-submitted directory data: refuse to dial into the local network") can be bypassed by the one address form its sibling module documents as the trap. Two implementations of one security rule, one of which has already drifted.

Remediation decision: Share URL host normalization and reserved-address screening between radio player and logos; test bracketed loopback, mapped IPv4, link-local and public literals. Keep DNS on workers.

Verification: Current source path confirmed via SymForge; regression/implementation pending.

Acceptance: Demonstrate the invalid-input or failed-I/O path is safe, the normal path still works, and relevant tests plus strict Clippy pass.

### W012

- Task: T006 [US1]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `crates/core/src/feedback/store.rs:113`.
- Dependencies: W011.

Claim: `FeedbackStore::write_atomic` is a third hand-rolled copy of the crate's crash-safe write, and it is the one copy that skips the Windows fallback. `crates/core/src/storage.rs:264-275` documents that "Win32 `rename` is not a guaranteed replace (`ERROR_ALREADY_EXISTS` on some volumes / rustc versions)" and provides `pub(crate) fn replace_file` (storage.rs:277) which falls back to `ReplaceFileW`; `AppConfig::save` (config.rs:884) and `BuildStorage::save_overwrite` (storage.rs:122) both route through it. `write_atomic` - which overwrites an existing `messages.json` on every dirty flush and `feedback_taxonomy.json` on every taxonomy refresh - calls bare `std::fs::rename` instead, even though it lives in the same crate and `replace_file` is `pub(crate)`.

Remediation decision: Use crate::storage::replace_file for atomic feedback and taxonomy publication, preserving temporary-file cleanup and the previous file on errors.

Verification: Current source path confirmed via SymForge; regression/implementation pending.

Acceptance: Demonstrate the invalid-input or failed-I/O path is safe, the normal path still works, and relevant tests plus strict Clippy pass.

### W001

- Task: T007 [US1]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `.github/workflows/ci.yml:17`.
- Dependencies: story entry gate.

Claim: The workspace CI gate cannot pass. I ran `cargo clippy --workspace --all-targets` on the current tree and it emits 10 warnings (news_art.rs:473 `1920u32` min/max no-op, radio/art.rs:276 manual is_multiple_of, radio/player.rs:449 large enum variant, tabs/radio.rs:350-352 sort_by_key x3 and :702-704 needless borrow x3, scraper.rs:1442 contains-vs-iter().any()). With `-D warnings` every push to main and every PR fails this step, so `cargo test --workspace` on line 18 never even runs. None of the 10 warning sites are in the files with uncommitted edits, so this is true at HEAD, not an artifact of local work. The plan that created this file (docs/superpowers/plans/2026-08-26-foundational-remediation.md:196) records 'clippy -D warnings clean workspace-wide' - that state has since regressed.

Remediation decision: Fix all ten observed Clippy diagnostics without relaxing CI; verify workspace Clippy including all targets.

Verification: All ten sites fixed in place (news_art downscale predicate + test, `is_multiple_of`, boxed `Deferred::Play`, `sort_by_key`, drop needless `t()` borrows, `KNOWN_SPECS.contains`). `cargo clippy --workspace --all-targets -- -D warnings` exits 0. CI workflow still uses `-D warnings`. news_art lib tests 13 passed; scraper luminary test passed.

Acceptance: Demonstrate the invalid-input or failed-I/O path is safe, the normal path still works, and relevant tests plus strict Clippy pass.

### B001

- Task: T008 [US2]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, observed; location: `crates/addon/src/ui/main_view/optimization.rs:1050`.
- Dependencies: story entry gate.

Claim: Choya copies validated per-slot prefixes but plates a uniform PvE prefix estimate with default modifiers. The accepted gate uses validated stats, so display and ranking disagree; rejected slot warnings are omitted from narrative.

Remediation decision: Pass the accepted ValidatedBuild into attach_chat_stats and use engine::calculate_validated_stats with balance context and returned modifiers. Surface validation corrections. Add mixed-slot independent-budget and PvP regressions.

Verification: `attach_chat_stats` now takes `Option<&ValidatedBuild>`. Present builds use `calculate_validated_stats` plus returned modifiers and `gear_quality_reasons`; warnings land on `quality_reasons` and in the chat bubble. `attach_chat_stats_uses_validated_mixed_slots` and `attach_chat_stats_pvp_uses_amulet_not_land_kit` passed.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W025

- Task: T009 [US2]. Status: **verified-scoped**; workspace/in-game gates pending.
- Audit: S2, confirmed; location: `crates/optimizer/src/gemini_tools.rs:1529`.
- Dependencies: story entry gate.

Claim: exec_simulate_rotation silently substitutes invented stats (2000 power / 1000 condition damage) when the gear prefix cannot be resolved, then reports the resulting DPS to the LLM as if it were real. Every sibling tool in the same file returns an explicit error JSON on the same failure (exec_calculate_stats line 839, exec_simulate_combat line 877, exec_score_build line 949); only this one fabricates. The response JSON built at lines 1588-1604 carries duration_s / dps / condition_uptime / skill_usage and no indication the stats were made up, and BUILD_DISCIPLINE in prompts.rs:139 explicitly tells the model 'If the numbers contradict the plan, change the plan — never the numbers.' The default prefix on line 1516 is the hardcoded English literal "Berserker's", so a missing gear_prefix argument depends on that exact name existing in GameDb.

Remediation decision: Return an explicit tool error when prefix/stat resolution fails; never invent a stat block.

Verification: Invented `(2000, 1000, 1100)` fallback removed. Unresolved/unpriceable prefixes return the same class of `error` JSON as `exec_calculate_stats`. `simulate_rotation_errors_when_prefix_cannot_be_priced` and `duration_seconds_is_clamped` passed.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W035

- Task: T010 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/rotation/simulator.rs:220`.
- Dependencies: W025.

Claim: `SimParams::basic` is a constructor whose defaults are documented as test-shaped — simulator.rs:192-193 says "`precision == 0` skips the crit term so existing CC-only tests keep the same strike numbers" — yet it is the only parameter source for the `simulate()` entry point, which is called on a shipping path. `grep -rn 'simulator::simulate' crates server` shows gemini_tools.rs:1541 calling `rotation::simulator::simulate(...)`, which routes through `simulate` -> `SimParams::basic` at simulator.rs:262. That constructor pins mode: GameMode::PvE, precision: 0.0, ferocity: 0.0, fury_crit_chance_bonus: 25.0 (the PvE value), max_health: 20_000.0 and armor: 2_000.0.

Remediation decision: Build SimParams from resolved stats and balance mode in the tool and call simulate_with; restrict the basic test convenience to actual test consumers.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W019

- Task: T011 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/objective_profiles.rs:86`.
- Dependencies: W044.

Claim: Five of this struct's six fields deserialize from `data/objective_profiles/*.json` and are then read by nobody. `grep -rn --include=*.rs '<field>' crates server` for `min_stunbreaks`, `requires_stability`, `min_cleanse_count`, `min_cleanse_rate_per_20s`, and `boon_uptime_floors` returns hits only inside this file (the declarations). The sole consumed field is `ehp_floor`, read at referee.rs:541 (`.and_then(|p| p.viability_gates.ehp_floor)`); grepping `viability_gates.` repo-wide returns only `ehp_floor` accesses.

Remediation decision: Implement profile viability requirements in the referee with failed/pass boundary tests; preserve mode-specific semantics.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W044

- Task: T012 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `data/normalized_effects/2026-01-13/pve.json:2`.
- Dependencies: story entry gate.

Claim: The patch-aware data layer has no normalized effects for the patch it declares active. data/manifests/2026-07-15.json says `"status": "active"` and data/manifests/2026-01-13.json says `"status": "superseded"`; balance_overrides and patch_ledgers both have a 2026-07-15 set, but data/normalized_effects/ contains only 2026-01-13/. crates/optimizer/src/balance.rs:12 pins `SNAPSHOT_PATCH_ID = "2026-07-15"`, while normalized_effects.rs:58-62 hardcodes `include_str!` of the three 2026-01-13 files, and the only production consumers (engine.rs:1274 and engine.rs:1607, plus rotation/wvw_timeline.rs:2663/2695) call `effects_for_mode()`, documented at normalized_effects.rs:397 as 'Effects for a game mode, ignoring snapshot patch_id mismatch'. The patch-aware accessor `effects_for(patch_id, mode)` has no production callers. The consistency test that should catch the gap hardcodes "2026-01-13" (data/consistency_tests.rs:111-131) instead of asserting against the active manifest id, so it passes by construction.

Remediation decision: Make normalized-effect patch mismatch explicit in production and active-manifest consistency validation; historical data must not be silently relabeled as current verified evidence.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W013

- Task: T013 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/balance.rs:11`.
- Dependencies: W044.

Claim: `SNAPSHOT_PATCH_ID` is a hand-edited date literal and the ONLY writer of `BalanceContext.patch_id`: `BalanceContext::new` (balance.rs:30-35) is the sole constructor, and grep -rn 'patch_id' crates server shows no other assignment. That string is then read at crates/addon/src/ui/main_view/tabs/saveload.rs:64, :333 and :388 and handed to `BalanceOverrides::lookup(patch_id, mode, ...)` (crates/optimizer/src/data/balance_overrides.rs:118-126), which does `self.files.get(&(patch_id.to_string(), mode.to_string()))?` — an exact-match key over the on-disk `data/balance_overrides/<patch>/{pve,pvp,wvw}.json` tree. That tree currently holds 2026-01-13/ and 2026-07-15/; only the latter is reachable, and the former is unreachable dead data. The module doc (balance.rs:6-7, 24) admits the sourcing is 'temporary ... until P3-08 adds manifest-backed authoritative sourcing'.

Remediation decision: Derive the active patch from manifests, retain explicit historical lookup, and make unsupported live-build/patch mismatch observable. Do not delete historical override data.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W015

- Task: T014 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/manifests.rs:43`.
- Dependencies: W013.

Claim: The patch-staleness warning is never delivered to anyone. `grep -rn --include=*.rs 'check_staleness' crates server` returns two hits: this definition and the `pub use manifests::{check_staleness, PatchManifest};` re-export at data/mod.rs:23. Nothing in crates/addon (which is where the live build number lives — `main.live_build_number` is used at crates/addon/src/feedback/tasks.rs:40) ever calls it. `latest_manifest()` is likewise only reached from consistency_tests.rs:470.

Remediation decision: Wire manifest staleness detection to the live build-number result and visible data status.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W016

- Task: T015 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/mod.rs:137`.
- Dependencies: W044.

Claim: The entire data-layer health check is unreachable from the shipping DLL. `grep -rn --include=*.rs 'initialize(' crates server` returns exactly three real hits: the definition (mod.rs:137), mod.rs:185 inside `#[cfg(test)] mod tests`, and consistency_tests.rs:972 inside `#[cfg(test)] mod consistency_tests`. No addon/optimizer runtime path calls it. Everything it exists to drive is therefore dead too: the ten `try_load_*` functions (mod.rs:140,143,146,149,152,155,158,161,164,167), the `try_load!` macro (mod.rs:99-117), the `DataLoadError` enum, and the `DataState` enum.

Remediation decision: Wire data initialization to startup and propagate disabled/degraded status; no success-quality fallback after a loader failure.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W039

- Task: T016 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:1389`.
- Dependencies: story entry gate.

Claim: `trigger_procs` matches on `EffectCategory` and handles 9 of the 22 variants declared at data/normalized_effects.rs:112-135 (StrikeDamagePct, the 7 status-operation categories, and OutgoingHealingPct behind a `duration_ms > 0` guard). The other 13 — FlatStat, StatConversion, ConditionDamagePct, SpecificConditionDamagePct, CritDamagePct, BoonDurationPct, ConditionDurationPct, SpecificConditionDurationPct, IncomingStrikeMultiplier, IncomingConditionMultiplier, DefianceDamage, ProcEffect, TriggeredEffect — plus OutgoingHealingPct with duration_ms == 0 fall into this catch-all, which does nothing and does NOT increment `unmodeled_effect_sources`. Every other unmodelled path in this file does increment it: load_normalized_effects:495 and :499, resolve_combo:1203, :1219, :1229, :1235, :1241, :1245, :1247.

Remediation decision: Count unsupported proc categories and zero-duration unsupported healing in coverage; test reporting without counting the same source repeatedly per tick.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W038

- Task: T017 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:861`.
- Dependencies: story entry gate.

Claim: The Protection strike multiplier is hardcoded as the literal 0.67 twice in wvw_timeline.rs (861 incoming, 1090 outgoing vs a Protection'd enemy), while the rest of the codebase reads the same number from data. `grep -rn 'protection_multiplier' crates` shows simulator.rs:653 and combat.rs:504 calling `boons().protection_multiplier()`, which resolves from data/formulas/boons.json with `.unwrap_or(0.67)` at boon_condition_formulas.rs:299 and is pinned by a test at boon_condition_formulas.rs:841.

Remediation decision: Replace both literals with `crate::data::boon_condition_formulas::boons().protection_multiplier()`, matching simulator.rs:653. If the per-tick call cost matters, resolve it once into a Timeline field in `Timeline::new`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W008

- Task: T018 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:714`.
- Dependencies: story entry gate.

Claim: Three independent parsers exist for the same ad-hoc "Heal: / Utils: / Utility: / Elite: / Pets: / Stances:" display-string format, and they disagree: `parse_skill_names` (714) splits `Utils: a, b, c` on commas and falls back to treating an unlabeled row as a skill name; `skill_selection_from_suggestion` (875) is case-insensitive, also splits `Utils:`, but silently drops unlabeled rows and knows nothing about `Pets:`; `gear_diff::parse_suggestion_skills` (crates/addon/src/ui/gear_diff.rs:19) is case-insensitive, knows `Pets:`/`Stances:`, does NOT understand `Utils:` (it would push the whole "Utils: a, b, c" string as one utility) and pushes unlabeled rows into utilities. The producers emit both spellings — `summarize_resolved_build` (976) writes `Utils:`, `gemini_from_validated` (1149) writes `Utility:`.

Remediation decision: Give `BuildSuggestion` typed fields (`heal: Option<SkillRef>`, `utilities: [Option<SkillRef>; 3]`, `elite`, `pets`, `stances`) and format the display strings from those at render time; delete all three parsers. Short of that, move one parser into `gear_diff` and have the other two call it. The verbatim-duplicated `strip_label_ci` helper (optimization.rs:880, gear_diff.rs:20, gear_diff.rs:61) goes with it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W004

- Task: T019 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/state.rs:898`.
- Dependencies: story entry gate.

Claim: Config and chat-history writes on shipping paths discard their `Result` entirely, while the crate has two working log channels for exactly this (`ui::log_disk_error`, `state::worker_log`) and `save_config_detached` (ui/mod.rs:170) does log the same failure. A disk-full, permission-denied, or locked-file failure silently discards the player's settings with no trace in the Nexus log.

Remediation decision: Route these through `if let Err(e) = ... { crate::ui::log_disk_error(format!("config save failed: {e}")) }`, matching `save_config_detached`. For `save_history`, log both the `to_vec` failure and the `rename` failure.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W009

- Task: T020 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/ui/main_view/tabs/settings.rs:60`.
- Dependencies: story entry gate.

Claim: 19 Settings/News click handlers persist config with `let _ = state.config.save(..)` — a synchronous disk write on the render thread under the STATE mutex whose Err is discarded. grep `config\.save(` across crates/addon/src: 17 `let _ =` sites in settings.rs + 2 in news.rs, versus settings.rs:238 which logs the error via nexus::log, and settings.rs:1016/1028/1048/1235 + every radio.rs site which use `crate::ui::save_config_detached` (ui/mod.rs:166, logs via log_disk_error and runs off-frame). ui/mod.rs:161-163 explicitly documents Settings as the remaining synchronous-save path. Three idioms for one operation in one file.

Remediation decision: Replace every `let _ = state.config.save(&state.config_path)` in settings.rs and news.rs with `crate::ui::save_config_detached(state)` (already used in the theme section of the same file); if a synchronous save is genuinely required somewhere, `if let Err(e) = .. { nexus::log::log(Warning, ..) }` as settings.rs:238 already does.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W082

- Task: T021 [US2]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/saveload.rs:509`.
- Dependencies: story entry gate.

Claim: When the ranch-load thread cannot be spawned, `load_named` falls back to spawning a second worker just to flush the dirty notes draft and discards that spawn's `bool` result. If the OS refused the first thread it will almost certainly refuse the second, so the player's edited notes are silently dropped: `pending_note_snapshot` (440-460) already mutated `saved_builds[..].notes` in memory, so the UI shows the new notes while disk keeps the old ones, and the error message set at 505-506 talks only about the load.

Remediation decision: Check the second spawn's result and, on `false`, either write the snapshot synchronously (it is a tiny JSON) or set `state.main.error` to say the notes were not saved; alternatively revert the in-memory notes mutation.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W182

- Task: T022 [US2]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:859`.
- Dependencies: story entry gate.

Claim: select_skills waives the template-palette gate whenever the whole skill_to_palette map is empty. `grep -rn 'skill_to_palette.is_empty' crates` shows this is the only such escape; search_v2's eligible_slot_skills (1341), swap_utility_skills (1498), swap_utilities_for_failed_gates (1626) and refill_bar (1862) all require palette_id != 0 unconditionally. The escape exists so make_diag_db fixtures (skill_to_palette: HashMap::new()) still pick utilities in optimize_synergy_wvw_selects_required_bar_utilities, while select_skills_skips_heals_without_template_palette has to insert a palette entry to make the gate fire.

Remediation decision: Drop the `is_empty() ||` clause and give the diag fixture palette entries (as the sibling test already does), so the seed and the beam apply one gating rule.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W225

- Task: T023 [US2]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/openai.rs:325`.
- Dependencies: story entry gate.

Claim: Malformed tool-call arguments from the model (typically a JSON string truncated by the completion cap) are silently replaced with `{}` and the tool is executed anyway, in openai.rs:325-326, openrouter.rs:343-344 and anthropic.rs:269. Nothing records that the arguments were unparseable; the only trace is whatever the tool returns for missing fields.

Remediation decision: On parse failure push a tool result of {"error":"unparseable arguments: <err>"} (so the model can retry) and count/report it the way sse.rs reports skipped payloads, instead of executing with empty args.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W157

- Task: T024 [US2]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:1109`.
- Dependencies: story entry gate.

Claim: Millisecond arithmetic on API-derived durations mixes saturating and non-saturating addition inside the same file. Bare `+` at wvw_timeline.rs:877, 890, 1109, 1147, 1261, 1274, 1280, 1312 and simulator.rs:727 sits alongside `saturating_add` at wvw_timeline.rs:998, 1124, 1173 and 1174 and simulator.rs:676, 847, 848 for the identical quantity. The durations come from builder.rs:377/388, which deliberately clamps with `duration.unwrap_or(0).saturating_mul(1000)` and is pinned by the test at builder.rs:857-879 whose comment says the pre-fix code 'overflowed u32 (panic in debug, silent wraparound in release)'. Cargo.toml has no `[profile]` section (`grep -n '^\[profile' Cargo.toml crates/*/Cargo.toml` returns nothing), so the shipped DLL builds with overflow-checks off.

Remediation decision: Use `saturating_add` for every `now_ms`/`current_time_ms` + duration expression, matching the sibling lines that already do. A one-line helper (`fn at(&self, offset_ms: u32) -> u32 { self.now_ms.saturating_add(offset_ms) }`) makes the whole file consistent and removes the choice.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W231

- Task: T025 [US2]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/sse.rs:317`.
- Dependencies: story entry gate.

Claim: `code` is a u64 taken verbatim from the provider's error object and truncated with `as u16`. A value above 65535 wraps silently (e.g. 65965 becomes 429 and is treated as RateLimited and retried by `is_retryable_status`). No panic and no realistic provider sends such a code, so severity is cosmetic; it is the only unchecked wire-to-integer cast in the batch.

Remediation decision: `u16::try_from(code).unwrap_or(502)` so an out-of-range code falls back to the generic 502 instead of wrapping.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W034

- Task: T026 [US2]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/referee.rs:937`.
- Dependencies: story entry gate.

Claim: `evaluate_validated_build` runs `calculate_combat_performance` three times (combat_solo L928, combat_party L937, combat_squad L946) but only the one selected by `scenario.combat_tier` is used — L957-959 clone it into `primary_combat`, and every gate, the flow sim, and all three scores read only `primary_combat` (L971, 986, 989, 997). The other two are moved into `RefereeReport` fields (L1053-1055) that nothing reads: `grep -rn --include=*.rs 'combat_party|combat_squad' crates server` shows RefereeReport's copies are only ever written (referee.rs:880-881 field decl, :938/:947 binding, :1054-1055 init, grouped_sheet.rs:383-384 init, and test fixtures at referee.rs:1259-1260 / :2442-2443 / search_v2.rs:2066-2067). Every read of `.combat_party` / `.combat_squad` in the addon (optimization.rs:126-127, :191-192) is against engine.rs's separate `OptimizationResult`, not RefereeReport.

Remediation decision: Compute only the scenario-selected combat tier during referee candidate evaluation and remove unused report fields after caller/fixture updates.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Reproduce the observed calculation/state discrepancy; check the corrected output against canonical or independent inputs and verify affected consumers.

### W003

- Task: T027 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/radio/player.rs:1357`.
- Dependencies: story entry gate.

Claim: `saved_from_station` and `station_from_saved` exist twice, field-for-field identical (including the `// A rehydrated favorite has no directory vote count.` comment and the `lastcheckok: 1` / `hls: 0` defaults): once as `pub fn` in player.rs:1357/1373 and once as private `fn` in crates/addon/src/ui/main_view/tabs/radio.rs:1580/1593. Neither copy calls the other — `grep -rn --include=*.rs 'player::saved_from_station|player::station_from_saved' crates server` returns nothing, so the UI file uses only its own unqualified copies (radio.rs:508, 1292, 1546) while player.rs uses its own (player.rs:458 in `toggle`, player.rs:782 when persisting `last_station`). Both copies even carry their own round-trip test (player.rs:1562, radio.rs:1702).

Remediation decision: Delete the two private copies in ui/main_view/tabs/radio.rs and import the already-`pub` `player::saved_from_station` / `player::station_from_saved` (the file already does `use crate::radio::{..., player, ...}`); keep one round-trip test.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W005

- Task: T028 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/ui/gear_sheet.rs:250`.
- Dependencies: story entry gate.

Claim: `render_resolved_sheet` (lines 250-413) and `render_suggestion_sheet` (lines 416-567) are two ~150-line parallel implementations of the same 3-column gear sheet: identical ARMOR_SLOTS loop, identical TRINKET_SLOTS loop, identical relic row, identical weapon-set loop, identical `piece_lock`/`weapon_lock`/`slot_tint`/`row`/`weapon_row` call shapes. They differ only in which side supplies the primary prefix and which supplies the `other` tooltip line. Any change to a gear row must be made twice.

Remediation decision: Collapse to one renderer parameterised by a small `SheetSide { primary_prefix, other_prefix, primary_name }` resolver (or a closure pair), so the armor/trinket/relic/weapon loops exist once. The source-scraping pin test can then be deleted in favour of a normal unit test on the resolver.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W007

- Task: T029 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/ui/main_view/lock_panel.rs:790`.
- Dependencies: story entry gate.

Claim: `render_optimized_specs_panel` (790-1064, ~275 lines) is a copy of `render_lock_panel` (214-741) with the interactions stripped. The two carry byte-identical copies of the layout maths (830-839 vs 281-290: `font_size/13.0`, `hex_radius`, `spec_hex_width`, `trait_area_width`, `col_spacing`, `row_height`, `circle_radius`), the hexagon + icon + "E" + slot-number + name-below block (871-925 vs 327-421), the 3x3 trait grid with `trait_idx = col * 3 + row` and the `selected_link` / `draw_ghost_link` pairing (937-1032 vs 471-654), and the row separator (1052-1062 vs 664-677).

Remediation decision: Extract one `paint_spec_row(ui, geometry, spec, trait_state, style)` that both call, with an enum or a small `interactive: bool` for the click/tooltip handling, and hoist the sizing block into a single `LockGeometry::for_width(ui, avail)`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W010

- Task: T030 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/addon/src/ui/mod.rs:212`.
- Dependencies: story entry gate.

Claim: A one-version config migration is still compiled into the hot render path. `LEGACY_FIRST_WINDOW_SIZE` is documented in crates/core/src/config.rs:624 as "First-run size before 1.7.22. Reset / missing size no longer uses this." The workspace is now at 1.11.26, and this exact-f32 equality check against [800.0, 600.0] runs on every frame's state read.

Remediation decision: Delete the `legacy` branch here and the `LEGACY_FIRST_WINDOW_SIZE` const, or move the one-shot upgrade into `AppConfig::load` gated on a stored config schema version so it can only fire once per install.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W014

- Task: T031 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/balance_overrides.rs:302`.
- Dependencies: story entry gate.

Claim: This function is unconditionally a no-op AND has no caller. (a) Every one of the five `KnownModeSplit` entries in `known_mode_splits()` (lines 240-283) sets `handled_in_phase_a: true`, and the loop's first statement is `if split.handled_in_phase_a { continue; }` — so lines 323-359, the entire lookup and both degradation branches, are unreachable and the function always returns `(DataQuality::Verified, vec![])`. (b) `grep -rn --include=*.rs 'check_wvw_quality' crates server` returns only the definition and the `pub use` re-export at data/mod.rs:19; nothing calls it.

Remediation decision: Remove the unused no-op check_wvw_quality and its re-export, including the unreachable placeholder lookup (W106), after proving current callers are absent.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W017

- Task: T032 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/normalized_effects.rs:522`.
- Dependencies: story entry gate.

Claim: This is a second, unused ~130-line effect scorer running in parallel with the live one. Its own doc says it "Produces values comparable to `synergy::score_normalized_effect()`" — and `synergy::score_normalized_effect` (crates/optimizer/src/synergy.rs:639) is the one with real callers (synergy.rs:668, :692 and downstream). `grep -rn --include=*.rs 'score_effect' crates server` returns only the definition, the `pub use` at data/mod.rs:25, and this file's own `#[cfg(test)]` tests. The helper `effect_uptime` and the two weight helpers `status_weight_for_scoring`/`cond_importance_from_op` exist solely to feed it.

Remediation decision: Keep the live synergy scorer and remove the unused parallel score_effect implementation and exclusive helpers/re-export after reference checks.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W018

- Task: T033 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/normalized_effects.rs:724`.
- Dependencies: story entry gate.

Claim: A ~200-line migration shim (lines 719-1020) with no caller. Its own doc says "Map a legacy 8-variant `synergy::NormalizedEffect` to the new 23-category `NormalizedEffect`. Useful for Phase 2 migration from old extractors." `grep -rn --include=*.rs 'map_legacy_effect' crates server` returns only the definition and the `pub use` at data/mod.rs:25. Its two private helpers `map_stat_type_to_hint` (line 987) and `map_damage_category` (line 1006) have zero references anywhere in the repo, including tests.

Remediation decision: Remove unused legacy-effect mapper and exclusive helpers/re-export after reference checks; retain any still-live synergy import.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W020

- Task: T034 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/patch_ledger.rs:17`.
- Dependencies: W013.

Claim: The whole `patch_ledger` module is consumed only by test code. `grep -rn --include=*.rs 'patch_ledger::ledgers\|ledger_for_patch\|LedgerChange\|load_ledger' crates server` returns: consistency_tests.rs:145 and :426 (both inside `#[cfg(test)]`), the `pub use patch_ledger::PatchLedger;` re-export at data/mod.rs:29, and this file's own tests. `ledger_for_patch` and `LedgerChange` have zero references outside the file. The module also embeds two YAML files via `include_str!` and is the *only* consumer of `serde_yaml` in the entire repo (`grep -rn --include=*.rs 'serde_yaml' crates server` -> patch_ledger.rs:34 and :61 only).

Remediation decision: Retain patch ledgers as test-time provenance: gate the module appropriately and move serde_yaml to dev dependencies if no runtime consumer remains. Preserve historical data files.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W021

- Task: T035 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/data/rotation_profiles.rs:119`.
- Dependencies: W019.

Claim: Three `RotationProfile` fields are parsed out of `data/rotation_profiles/{pve,pvp,wvw}.json` and never read: `boon_generation` (line 119), `incoming_suppression` (line 125), and `objective_profile_id` (line 117). `grep -rn --include=*.rs 'boon_generation\|incoming_suppression' crates server` returns hits only in this file's declarations — zero reads. For `objective_profile_id`, every repo hit is on the *ObjectiveProfile* or *Scenario* struct field of the same name (objective_profiles.rs:247/264, scenario/optimize_flow/examples); `profile.objective_profile_id` is never read off a `RotationProfile`. The entire `GenerationMetrics` enum (line 71, two variants) exists only to type `boon_generation`, and is otherwise referenced only by the `pub use` at data/mod.rs:33.

Remediation decision: Wire supported rotation profile controls to their intended calculations; validate objective profile references and reject unsupported controls explicitly instead of silently ignoring them.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W022

- Task: T036 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/engine.rs:70`.
- Dependencies: story entry gate.

Claim: Two public non-cancellable wrapper functions are kept alive by doc comments that name a production call site which has already migrated away. `grep -rn --include=*.rs 'engine::optimize\b|optimize_deterministic\b' crates server` finds the addon calling only the cancellable variants (optimize_flow.rs:587 `optimize_deterministic_cancellable`, optimize_flow.rs:617 `optimize_cancellable`), plus a regression test at optimize_flow.rs:1643-1647 that asserts production source never contains `engine::optimize(` or `engine::optimize_deterministic(`. SymForge find_references on `optimize_deterministic` returns only those two optimize_flow.rs anchors — i.e. zero real callers anywhere, tests included. `optimize` (engine.rs:80) is called only from engine.rs's own `#[cfg(test)]` module (lines 3296, 3569, 3649, 3697, 3738, 3787, 3839). Both doc blocks still carry a bolded imperative — '**that call site must move to `optimize_cancellable`**' (line 75) and '**that call site must move to `optimize_deterministic_cancellable`.**' (line 1920) — describing work that is already done, and both are justified by 'which this change does not own', an ephemeral PR-scope excuse frozen into permanent API documentation.

Remediation decision: Remove unused production wrappers; port affected tests to cancellable entry points or retain an explicitly test-only convenience.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W023

- Task: T037 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/engine.rs:1103`.
- Dependencies: story entry gate.

Claim: Doc comments belonging to deleted functions were left in place and are now anchored on whatever item followed them, so rustdoc attributes them to the wrong symbol. At engine.rs:1103-1117, three separate doc blocks — 'Stage 3 of the Gemini pipeline: assemble the final synergy prompt', 'Stage 4 of the Gemini pipeline: call the LLM with tool definitions', and 'Run the synergy-driven optimization pipeline. / Sends ALL profession data to Gemini in a single prompt' — all stack onto `pub fn calculate_validated_stats`, a pure stat-sheet computation that makes no LLM call and assembles no prompt. Same defect at engine.rs:1765-1770, where `add_weapon_skill_ids`'s three-line doc ('Add weapon skill IDs for a given weapon type…Land Spear stays') sits on `enum Hand`; at engine.rs:2278-2286, where `llm_advisor`'s doc ('Post-beam LLM advisor: ask the LLM for candidate mutations…') sits on the two-argument `advisor_rune_pick`; and at validation.rs:430-435, where `validate_gemini_build`'s doc ('Validate a parsed Gemini build response against the GameDb. / Always returns a ValidatedBuild, even if there are errors.') sits on `infer_profession_from_spec_names`, leaving the real entry point at validation.rs:499 with no doc at all. validation.rs:304-318 is a fifth variant: the same 'Resolve migration-produced zero itemstat ids' paragraph is written twice, before and after an interleaved TODO. Additionally, the (correctly placed) part of the engine.rs:2282 line asserts 'LLM errors are silently logged' — the optimizer crate contains no `log::` call at all (`grep -rn 'log::' crates/optimizer/src` returns nothing).

Remediation decision: Delete the orphaned blocks at engine.rs:1103-1116, engine.rs:1765-1767, engine.rs:2278-2284 and validation.rs:430-433, keeping only the doc line that actually describes the following item; move `validate_gemini_build`'s doc down to validation.rs:499; collapse the duplicated paragraph at validation.rs:304-307 vs 312-318 into one; and fix 'silently logged' to state what the code does (see the swallowed-error finding).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W024

- Task: T038 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/gamedb.rs:655`.
- Dependencies: story entry gate.

Claim: `traits_granting_buff` and `skills_granting_buff` have zero callers anywhere: grep -rn --include=*.rs 'traits_granting_buff\|skills_granting_buff' crates server returns only the definitions in gamedb.rs (655-668), and SymForge find_references reports 'No references found' for both. They are the only readers of the `traits_by_buff` / `skills_by_buff` fields (grep confirms the only other hits are the declarations at gamedb.rs:41-42, the population loops at :207 and :227, the dedup loops at :242 and :250, and the struct literal at :293-294). So `GameDb::load` walks every trait's facts and every skill's facts, allocates two String-keyed HashMaps, then sort_unstable+dedup every bucket, purely to fill indexes nothing reads.

Remediation decision: Remove unused buff indexes, population/dedup work and accessors once fresh reference queries confirm no consumers.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W026

- Task: T039 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/gemini_tools.rs:1913`.
- Dependencies: W032.

Claim: This is the third independent parser of GW2 rune-bonus strings in the optimizer crate. `grep -rn --include=*.rs 'fn parse_rune' crates` returns three: combat.rs:1046 parse_rune_modifier (-> DamageModifiers, via parse_percent_clauses), synergy.rs:452 parse_rune_bonus_to_effects (-> NormalizedEffect), and this one (-> JSON for the LLM). All three tokenize the same unstructured API text ("+7% Burning Duration", "+175 Power", "+5% damage") with three different hand-rolled scanners; this one also carries its own number extractor, extract_number at line 1994. Unlike the other two it never strips GW2 markup (combat.rs uses strip_gw2_markup) and has no "all stats" case (synergy.rs parse_all_stats_bonus).

Remediation decision: Extract one tokenizer (markup strip + number/percent/stat-name extraction) into a shared module and have all three call sites map its output into their own result type, rather than each re-scanning the raw string.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W027

- Task: T040 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:373`.
- Dependencies: W030.

Claim: anthropic.rs:371-446 re-implements the retry loop of openai_compat::send_chat (openai_compat.rs:304-394) step for step: same cancel check, same `sleep_observing`/`doubled_backoff` dance, same 200/401/retryable/other status match, same in-band-retryable-error arm, same `last_error.unwrap_or_else(500 'server error after retries')` tail. It imports the helpers from openai_compat but not the policy. The literal `Duration::from_secs(5)` duplicates the private `INITIAL_RETRY_DELAY` (openai_compat.rs:43) and `const MAX_RETRIES: u32 = 3` (anthropic.rs:340) silently diverges from the `max_retries: 2` the other two providers use. The 'in-band retryable error' arm was evidently patched into both copies separately (openai_compat.rs:356-366, anthropic.rs:415-422).

Remediation decision: Extract shared retry policy using provider-specific request/reader closures; retain intentional per-provider retry limits with documentation and transport regression tests.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W028

- Task: T041 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/llm/mod.rs:159`.
- Dependencies: W030.

Claim: LlmClient::generate_cached and LlmClient::clear_cache have no production caller, so the entire response cache (response_cache.rs, the `cache` field on OpenAiClient/OpenRouterClient/AnthropicClient, six `ResponseCache::new(1800, 64)` literals, and the Grok F12 one-entry eviction fix) ships dead in the DLL. grep -rn 'generate_cached(' crates server -> only the trait decl, the three impls, the Gemini adapter delegating to crate::gemini (whose own generate_cached likewise has no caller), and tests/live_llm.rs. grep -rn 'clear_cache(' -> decl, impls, gemini adapter only; the Settings 'Clear cache' button (crates/addon/src/ui/main_view/tabs/settings.rs:1419-1424) clears gw2_api::cache::DataCache, not this. engine.rs and the addon call generate / generate_brief / generate_with_tools_progress only.

Remediation decision: Remove unused response-cache trait methods and provider state after current reference analysis; update live tests to exercise shipping generate methods.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W029

- Task: T042 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/llm/mod.rs:189`.
- Dependencies: W030.

Claim: `remaining_quota` has no production caller. `grep -rn remaining_quota crates server` hits only: the four provider impls in this batch, crate::gemini's own impl and its tests, crates/optimizer/tests/live_llm.rs:197, and engine.rs:2897 which is a `StubAdvisor` mock inside `#[cfg(test)] mod tests` (opens at engine.rs:2519). Nothing in crates/addon calls it, and `grep -rni quota crates/addon/src` finds no quota/requests-remaining widget. Yet the trait doc (mod.rs:181-188) and rate.rs:23-27 both describe a Settings UI field that 'reads as fact for all four (GLM F22)'. Downstream, `RateTracker::remaining_today` (rate.rs:145), `DISPLAY_DAILY_BUDGET` (rate.rs:28) and the persisted `day`/`requests_today` counters exist only to feed this method.

Remediation decision: Unify quota display with the persisted usage reader that Settings actually uses; remove uncalled quota trait methods and misleading UI claims without discarding rate-limit enforcement.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W030

- Task: T043 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/llm/openai.rs:267`.
- Dependencies: story entry gate.

Claim: openai.rs:267-347 and openrouter.rs:285-365 are a byte-identical 80-line tool loop (same comments, same `trim_openai_messages` call, same arg-parse fallback, same 'Tool loop exceeded' error), differing only in the provider name inside two error strings. The `with_persistence` usage-file loader is copied verbatim three times (openai.rs:54-64, openrouter.rs:68-78, anthropic.rs:311-321), and `generate`/`generate_brief`/`generate_cached`/`validate_key` are near-copies between openai.rs and openrouter.rs. openai_compat.rs:5-7 claims 'the wrappers add only what makes them distinct', which is contradicted by the wrappers. The identical 'Between turns as well as inside the stream' comment block in all three files shows a cancellation fix that had to land three times.

Remediation decision: Consolidate the OpenAI-compatible tool loop and shared wrapper policy in openai_compat; preserve provider URLs, headers, reasoning settings and model filters.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W031

- Task: T044 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/llm/openai.rs:267`.
- Dependencies: W030.

Claim: openai.rs and openrouter.rs are near byte-identical wrappers: generate_with_tools_progress (openai.rs:267-347 vs openrouter.rs:285-365, 80 lines differing only in the provider name string), plus new/with_persistence, validate_key, validate_key_detailed, generate, generate_brief, generate_cached, list_models status handling, remaining_quota and clear_cache. Diffed by reading both files in full; the only real differences are the base URL, two identity headers, REASONING_TOKEN_CAP, supports_provider_prefs, and list_models filtering. openai_compat.rs:5-7 claims 'the wrappers add only what makes them distinct', which is not what the files contain.

Remediation decision: Consolidate with W030; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W032

- Task: T045 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/parser_consistency_tests.rs:3`.
- Dependencies: story entry gate.

Claim: This module's own doc is the codebase admitting to two independently-maintained implementations of GW2 Fact interpretation, and records that they 'have silently diverged before (condition-damage dropped in one, \"Poison\" vs \"Poisoned\" key mismatch, first-vs-closest percent branch)' (lines 11-14). The only thing holding them together is a 16-row corpus in this test file (lines 88-197) run through two `#[cfg(test)] pub(crate)` shims (`combat::tests_consistency_shim::classify_fact`, `synergy::tests_consistency_shim::classify_fact`). The corpus covers only `Fact::Percent`; every other Fact variant the two parsers handle is unguarded.

Remediation decision: Extract a production Fact classification core consumed by combat and synergy, preserving independent output projection tests across handled Fact variants.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W033

- Task: T046 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/prompts.rs:401`.
- Dependencies: story entry gate.

Claim: build_game_context (lines 401-480, ~80 lines) has no production caller anywhere. `grep -rn --include=*.rs 'build_game_context' crates server` returns only its definition at prompts.rs:401 and two calls inside its own #[cfg(test)] module (prompts.rs:1021 and 1026); SymForge find_references reports no indexed callers. Its first parameter is already dead (`_profession`). The body holds the only copy of the WvW/PvP/PvE mode rules, including a 'VIABILITY GATES (treated as hard failures by the deterministic referee)' block (lines 441-446) asserting that 0 stunbreaks / no Stability / no cleanse are hard non-viable — text that reaches no LLM and no referee.

Remediation decision: Remove unused build_game_context and its private tests after moving any still-required mode discipline into the live prompt builders.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W036

- Task: T047 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/rotation/skill_timings.rs:18`.
- Dependencies: story entry gate.

Claim: Cast times have two live sources of truth. builder.rs:147-148 reads `sourced_skill_u32(ctx, skill.id, "activation_ms")` from data/balance_overrides/<patch>/<mode>.json and only falls back to this hardcoded 18-row table. The two overlap: skill 13097 (Heartseeker) is hardcoded here at line 21 as `SkillTiming::new(750, 0)` and also appears in data/balance_overrides/2026-07-15/wvw.json line 21 as `"activation_ms": { "value": 750, ... }` (same in pve.json and pvp.json). Each 2026-07-15 file has 3 `activation_ms` rows; `grep -c activation_ms data/balance_overrides/2026-01-13/*.json` returns 0 for all three, so for the 2026-01-13 patch this table is the only path.

Remediation decision: Consolidate sourced activation timings in balance override data while preserving explicit aftercast semantics and fallback evidence; test overlapping skills before removing the table.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W037

- Task: T048 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:357`.
- Dependencies: W039.

Claim: Seven Timeline fields are declared, initialised once to their identity value, multiplied into nine damage/heal/duration expressions, and NEVER assigned anywhere else. `grep -n '<field>' crates/optimizer/src/rotation/wvw_timeline.rs` for each of passive_strike_mult, passive_condition_mult, passive_healing_mult, incoming_strike_mult, incoming_condition_mult, bonus_boon_duration, bonus_condition_duration returns exactly three kinds of hit: the struct field (357-363), the initialiser in Timeline::new (456-462), and read sites in the math. No write site exists. `grep -rn` over crates+server finds no external writer either (the fields are private).

Remediation decision: Remove unwritten identity fields only after tracing passive parameter folding and preserving coverage accounting in W039.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W040

- Task: T049 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/scoring.rs:400`.
- Dependencies: story entry gate.

Claim: `ObjectiveScorer` (struct L400-418 + impl L420-503, ~105 lines: from_mode, from_profile, fallback, score, boon_priority, condition_priority, interaction_priority) has no production caller anywhere. SymForge find_references reports 50 references in exactly 2 files: scoring.rs itself (definition + its own #[cfg(test)] mod at L1829, 1838, 1847, 1861, 1893, 1934, 1937, ...) and crates/optimizer/tests/objective_profiles_integration.rs (L12, 307, 327, 342, 358, 381, 383, 399, 419, 421, 437, 463, 567, 592). `grep -rn --include=*.rs 'ObjectiveScorer' crates server` agrees: no hit in any src/ file other than scoring.rs. The consts BOON_SUPPORT_NORM (L22) and CONTROL_NORM (L24) exist solely to feed `ObjectiveScorer::fallback` (L460, L463) and are therefore transitively dead too.

Remediation decision: Remove the uncalled ObjectiveScorer twin after reference checks; preserve live calibrated score_with_weights constants and regression expectations.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W041

- Task: T050 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/scraper.rs:952`.
- Dependencies: story entry gate.

Claim: `extract_gear_prefix` is a second, hand-maintained implementation of `scoring::prefix_named_in_text` (crates/optimizer/src/scoring.rs:933). Both build the same ' '-padded ASCII-alnum haystack (scraper.rs:933-946 `padded_alnum_words` is byte-for-byte the `hay` expression at scoring.rs:934-945), both stem with `.trim_end_matches("'s")`, and both probe `" {stem} "` / `" {stem}s "`. The only difference is the name table: scoring.rs iterates the canonical `GEAR_PROFILES` (scoring.rs:845, 23 entries), while scraper.rs:954-979 hardcodes its own 24-entry list. The two have already drifted — the scraper list carries both "Trailblazer" and "Trailblazer's" (redundant, the stem trim already folds them) and "Sinister"/"Marauder"/"Valkyrie"/"Dragon's"/"Grieving" ordering differs, so a prefix added to GEAR_PROFILES is invisible to the scraper.

Remediation decision: Delete the local `prefixes` array and `padded_alnum_words`, make `scoring::prefix_named_in_text` public to the crate (it is already `pub`), and call it: `extract_gear_prefix(html)` becomes `prefix_named_in_text(html).unwrap_or_default().to_string()`. The negation skip (`stem_negated`) is harmless on HTML.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W042

- Task: T051 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/synergy.rs:111`.
- Dependencies: story entry gate.

Claim: NormalizedEffect::BenefitsFromStatus, NormalizedEffect::ProcEffect and the whole ProcTrigger enum (7 variants) are never constructed anywhere in the repo. `grep -rn 'estimated_uptime\|BenefitsFromStatus {\|ProcTrigger' crates server` finds only match arms: synergy.rs scoring (666, 694), synergy rule 2 Enabler→Payoff (752, 780), data/normalized_effects.rs map_legacy_effect (843, 943), upgrade_graph.rs tag builder (430). No extractor emits them, so SynergyLinkType::EnablerPayoff can never be produced and every 'benefits from status' scoring path is unreachable.

Remediation decision: Remove unconstructible legacy effect variants and exclusive rules after normalized mapper cleanup; preserve live normalized proc support.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W043

- Task: T052 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `crates/optimizer/src/text_util.rs:14`.
- Dependencies: W032.

Claim: `extract_percent_before` is gated `#[cfg(test)]`, so it is not compiled into the DLL at all — yet its regression test at text_util.rs:290-301 asserts 'The shared closest-percent implementation now protects BOTH the combat path and the synergy path against this regression.' It protects neither. grep -rn 'extract_percent_before' crates server returns hits only inside text_util.rs itself (the definition plus 8 test call sites); find_references agrees. The real production percent handling lives in `combat::parse_percent_clauses` / `classify_percent_text` (crates/optimizer/src/combat.rs:1051, :825) and in synergy.rs, neither of which imports this function — combat.rs:12 imports only `capitalize, stack_multiplier, strip_gw2_markup`.

Remediation decision: Either delete `extract_percent_before` and its 6 tests outright, or drop the `#[cfg(test)]` gate and make combat.rs/synergy.rs actually call it — then the comment becomes true. Do not leave it as-is. If deleted, move the closest-percent regression assertion onto whichever production function now owns that behaviour.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W045

- Task: T053 [US3]. Status: **planned**.
- Audit: S2, confirmed; location: `docs/architecture.md:21`.
- Dependencies: story entry gate.

Claim: The documented 3-tier optimization pipeline names two functions that do not exist anywhere in the repository. `grep -rn --include=*.rs 'optimize_with_gemini' crates server` and the same for `enrich_with_gemini` both return zero hits. Only tier 1 (`engine::optimize_deterministic` at engine.rs:1922) and tier 3 (`engine::optimize` at engine.rs:80 / `optimize_cancellable` at :119) exist; the addon calls `optimize_deterministic_cancellable` (optimize_flow.rs:587) and `optimize_cancellable` (:617). The identical false claim is repeated in CLAUDE.md, which is the file every agent and new contributor is told to read first.

Remediation decision: Re-derive the tier list from the actual call sites in crates/addon/src/ui/main_view/optimize_flow.rs and update both docs/architecture.md:20-22 and CLAUDE.md:30/37-41 with the real entry points (optimize_deterministic_cancellable, optimize_cancellable) and whatever the LLM tier is called now.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: One verified policy/implementation remains; production callers and compatibility are preserved; affected behavioral tests and strict Clippy pass.

### W046

- Task: T054 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `.gitignore:32`.
- Dependencies: story entry gate.

Claim: The tracked ignore rule for SymForge scratch directories is root-anchored, but the tool writes them inside the source tree: `find . -type d -name .symforge` returns ./.symforge, ./crates/addon/src/ui/.symforge, ./crates/addon/src/ui/main_view/.symforge, ./crates/optimizer/src/.symforge and ./crates/optimizer/src/llm/.symforge, and those contain generated .rs copies (e.g. crates/addon/src/ui/main_view/.symforge/tee/1787937928928-000000-build_display.rs). `git check-ignore -v` shows the nested ones are excluded only by `.git/info/exclude:30: **/.symforge/` - a machine-local file that is not in the repository, so on any other clone or for any other contributor those directories show up as untracked source files ready to be committed by `git add crates/`.

Remediation decision: Change the rule in .gitignore to un-anchored `.symforge/` (matching the `.git/info/exclude` pattern) so every clone ignores the nested directories.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W047

- Task: T055 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/lib.rs:53`.
- Dependencies: story entry gate.

Claim: The doc says `BOOTSTRAP_FAILED` exists so "the next PostRender can retry". The code does the exact opposite: `attach_overlay_host` returns immediately when the flag is set (lines 69-71), and the log message on that same path says "Overlay chrome attach panicked; will not retry this session." (line 100). The flag is a permanent latch that prevents retry, not one that enables it.

Remediation decision: Rewrite the doc to match the code: BOOTSTRAP_FAILED latches a panicked attach so the PostRender bootstrapper stops retrying for the session (a retry loop would re-panic every frame); recovery requires an addon reload.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W048

- Task: T056 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/news.rs:183`.
- Dependencies: story entry gate.

Claim: `fetch_body` collapses five distinct failures — client build failure, DNS/TLS/connect error, non-2xx status, body-over-cap, and invalid UTF-8 — into a bare `None`, and nothing anywhere in the news path logs. The whole module contains zero `nexus::log` calls (`grep -rn 'log::' crates/addon/src/news.rs crates/addon/src/news_art.rs` is empty), unlike radio/player.rs which routes failures through `radio_log` and feedback/tasks.rs which uses `nexus::log::log`/`crate::ui::log_disk_error`. `kick` turns the `None` into `note_fetch_failure` (news.rs:157), which sets only a 45 s backoff stamp; `NewsState` has no error field (contrast `RadioUiState::last_error`), so ui/news_feed.rs:30-41 renders the generic `view.empty` string. A 500 from the GW2 forum, an expired TLS chain, and a genuinely empty feed are indistinguishable to both the player and the developer.

Remediation decision: Return the failure instead of dropping it: make `fetch_body` return `Result<String, String>` (the shape `radio::directory::fetch` already uses at directory.rs:252) and have `kick` log the reason once per failed source via `nexus::log::log(LogLevel::Warning, ...)`, optionally storing it in a per-source error slot so the empty state can say why.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W049

- Task: T057 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/news_art.rs:394`.
- Dependencies: story entry gate.

Claim: News stills are uploaded as Nexus textures and written to `cache/news` with no bound of any kind, while the sibling module written against the identical constraint declares those bounds "non-negotiable". radio/logos.rs:8-21 states: "Nexus never frees textures until game exit (nexus-rs issue #138), so a long session must not upload logos without bound", and enforces `MAX_TEXTURES = 200` before `get_texture_or_create_from_file` (logos.rs:200-208) plus `MAX_CACHE_FILES = 500` with `evict_oldest` after every write (logos.rs:360). `news_art::texture` calls `get_texture_or_create_from_file` (line 415) with no budget gate, and `news_art::download` writes into `cache/news` (lines 320-324) with no eviction — `grep -rn 'evict' crates/addon/src` matches only logos.rs. Each still is kept at up to 1024px on its longest edge (MAX_EDGE), so every distinct image ever scrolled past in a session stays resident until the game exits, and the disk folder grows for the life of the install.

Remediation decision: Reuse the logos discipline in news_art: a `CREATED` set gated by a `MAX_TEXTURES` cap before `get_texture_or_create_from_file`, and a call to a shared `evict_oldest(dir, MAX_CACHE_FILES)` after the tmp+rename write in `download`. `evict_victims` in logos.rs:388 is already pure and unit-tested — lift it to a shared helper rather than copying it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W050

- Task: T058 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/radio/art.rs:6`.
- Dependencies: story entry gate.

Claim: The module doc states as an invariant that every blit is hard-clipped to the player-bar rect so nothing in the module "can ever paint over the station list". That stopped being true when the quip bubble moved to the foreground draw list: `draw_quip_bubble` opens with `let dl = ui.get_foreground_draw_list();` (art.rs:516) under the comment "the bubble renders on top of EVERYTHING" (art.rs:513-515), and the foreground list is not subject to the `dl.with_clip_rect_intersect(...)` scope that `draw_dj_choya` establishes at art.rs:339. Everything the bubble draws — plate, border, tail, text, the ON-AIR badge and the emote drip at art.rs:665-676 — deliberately paints over the station list. The change is recorded in the log as `8d0b9a2 quips: bubble on top of everything`; the module doc was never updated.

Remediation decision: Reword the module doc to say the sprite blits are clipped to the bar rect while the quip bubble deliberately renders on the foreground draw list above everything, and note the `right_limit` clamp at art.rs:576-579 as the actual mechanism that keeps the bubble off the hearts column.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W051

- Task: T059 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/radio/art.rs:492`.
- Dependencies: story entry gate.

Claim: The rustdoc on `draw_quip_bubble` describes an anchor roulette of "three leans above the head, or muffled mumbling down by the feet at 60% alpha". The code implements five anchors (`let anchor = (h >> 8) % 5;`, art.rs:521) that are all above the head, and the inline comment immediately below says so explicitly: "ALWAYS above the head (a low \"muffled\" slot used to land inside the now-playing ticker)" (art.rs:518-520). No alpha reduction for a muffled slot survives anywhere in the function — its only residue is the now-pointless alias `let a = alpha;` at art.rs:522, which used to be where the 60% factor was applied. The same file carries a second stale reference: the rect-table comment at art.rs:23 lists "DANCE[0], ONAIR, ZZZ and EQ_WARM rects ... see the comments on each", but no `EQ_WARM` const exists anywhere in the repo (`grep -rn EQ_WARM crates server` matches only that comment), because the sprite EQ strip was removed — see art.rs:679-680, "The sprite EQ strip is gone".

Remediation decision: Rewrite the `draw_quip_bubble` doc to describe the five above-the-head anchors actually implemented, inline `alpha` and delete the vestigial `let a = alpha;` binding, and drop `EQ_WARM` from the art.rs:23 rect-table note.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W052

- Task: T060 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/radio/player.rs:1069`.
- Dependencies: story entry gate.

Claim: This three-line doc comment belongs to `finish_stopped` (defined at player.rs:1094) but is attached to `fn stream_host_reserved` (player.rs:1077) — the two doc blocks were concatenated with no blank line or item between them, so lines 1069-1071 and lines 1072-1076 form one rustdoc for the reserved-host predicate. `cargo doc` therefore renders `stream_host_reserved` as "Stop-flag exit path: drop the sink handle, write `Stopped`, return...", and `finish_stopped` ships with no documentation at all despite being the function that actually performs that teardown.

Remediation decision: Move lines 1069-1071 down to sit directly above `fn finish_stopped` at player.rs:1094, leaving lines 1072-1076 as `stream_host_reserved`'s doc.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W053

- Task: T061 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/state.rs:530`.
- Dependencies: story entry gate.

Claim: `MainState::names_lang` is written but never read. A full-repo grep (`grep -rn 'names_lang' .` excluding .git/target/.symforge) returns exactly three hits: this declaration, `state.main.names_lang.clear()` at ui/main_view/stats.rs:116, and `s.main.names_lang = lang.clone()` at stats.rs:163. No comparison, no render, no branch consumes it. Its sibling `names_stage` is genuinely read at ui/main_view/mod.rs:290.

Remediation decision: Delete the field and its two write sites in stats.rs, or wire it into the localized-names guard if that was the intent (stats.rs:163 sets it right where such a guard would belong).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W054

- Task: T062 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/state.rs:830`.
- Dependencies: story entry gate.

Claim: The final `else => Screen::Setup(SetupStep::Gw2ApiKey)` arm (line 840) is unreachable: branch 2 already returns for `!has_gw2_key()`, so by branch 4 `has_gw2_key()` is necessarily true and always matches. For the same reason the `config.has_gw2_key() &&` conjunct in branch 3 is always true and redundant.

Remediation decision: Drop the redundant `config.has_gw2_key() &&`, and replace the dead `else` arm by making the `has_gw2_key()` branch the `else` (or leave one arm and delete the other). Rename the test to match what it asserts.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W055

- Task: T063 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/comparison.rs:557`.
- Dependencies: W083.

Claim: A format-sniffing compatibility shim for old saved builds, deciding between two incompatible unit conventions by testing the value itself. The two domains overlap on [0.0, 1.0], so the shim is ambiguous by construction and cannot be made correct.

Remediation decision: Normalise once at load (`SavedBuild` deserialization) using a stored schema/version field rather than sniffing, then delete this function and have the renderer just multiply by 100.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W056

- Task: T064 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/comparison.rs:1171`.
- Dependencies: story entry gate.

Claim: A stale doc comment for a deleted feature ("Legacy synthetic tradeoff report") is now attached to the `#[cfg(test)] mod tests` declaration. `grep -rn 'tradeoff' crates` finds no such report anywhere in the file or crate - the item it documented was removed and the `///` line was left behind, silently re-binding to the next item.

Remediation decision: Delete the line.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W057

- Task: T065 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/gear_diff.rs:20`.
- Dependencies: W008.

Claim: `strip_label_ci` is defined twice in this 164-line file, byte-for-byte identical including its 4-line safety comment: once nested inside `parse_suggestion_skills` (lines 20-30) and again inside `parse_suggestion_weapons` (lines 61-71).

Remediation decision: Hoist a single private `fn strip_label_ci` to module scope and call it from both parsers.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W058

- Task: T066 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/gear_sheet.rs:344`.
- Dependencies: story entry gate.

Claim: The relic row's slot label is passed as the English literal `"Relic"` at gear_sheet.rs:344 (Current sheet) and :502 (Optimized sheet), while every neighbouring row goes through `slot_label(slot)` -> `t("slot.helm")` etc. The key `"slot.relic": "Relic"` already exists in the catalogs (locales/en.json:287, and in all 12 locale files), and `grep -rn 't("slot.relic")' crates` returns nothing - the translation is shipped and never used.

Remediation decision: Add `"Relic" => "slot.relic"` to `slot_label`'s match and pass `slot_label("Relic")` at both call sites (note `row` takes `slot: impl AsRef<str>`, so `&String` works).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W059

- Task: T067 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/character.rs:37`.
- Dependencies: story entry gate.

Claim: User-facing error and status text in this batch is a mix of catalogue lookups and raw English literals. The same `state.main.error` field is fed by `t("err.no_gamedb")` / `t("err.no_character")` (optimize_flow.rs:79-89) and by hardcoded English here and at stats.rs:324, :342, :468, character.rs:81 (and its equipment twin), plus the status-bar text `format!("Refreshing: {} ({})", ...)` at stats.rs:267/269 which renders straight into the localized top bar (mod.rs:281-286). Excludes the two English constants already carrying a `// ponytail:` note.

Remediation decision: Add `err.no_api_key`, `err.refresh_failed`, `err.gamedb_load`, `err.cache_write` and `status.refreshing` to the catalogue and route these through `t`/`tf`, keeping the underlying error text as the interpolated detail.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W060

- Task: T068 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/lock_panel.rs:79`.
- Dependencies: story entry gate.

Claim: `spec_hex_width` sizes the spec column against the literal English elite-spec name "Dragonhunter", while the names actually rendered into that column go through the localization layer — `db.loc_spec(id, &s.name)` at lines 311/318 and 918. Both `render_lock_panel` and `render_optimized_specs_panel` size themselves from this one function (285, 835).

Remediation decision: Measure the widest localized name actually being rendered this frame (the spec names already resolved for the three slots), or take a `&str` parameter and pass the longest of them.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W061

- Task: T069 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/lock_panel.rs:869`.
- Dependencies: story entry gate.

Claim: This panel paints itself with literal RGBA arrays instead of the runtime theme palette the rest of the addon uses. `theme::OPTIMIZED` already exists and is a different green (`[0.55, 0.92, 0.62, 1.0]`, crates/addon/src/ui/theme.rs:16); this file re-invents it at 818, 869 and 1042, plus literal panel/separator colors at 811 (`[0.10, 0.19, 0.12, 0.9]`), 878 (`[0.05, 0.2, 0.1, 0.6]`) and 1059 (`[0.1, 0.25, 0.1, 0.3]`). The file already knows better — `locked_color()` at line 18 correctly reads `theme::pal().gold` — and the four module consts at 12-15 are the same gap.

Remediation decision: Replace the literals with `theme::OPTIMIZED` / `theme::pal()` entries, adding palette slots for the panel fill and separator if none fit; do the same for `SELECTED_COLOR` / `DIM_COLOR` / `AVAILABLE_COLOR` / `ELITE_COLOR` at 12-15.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W062

- Task: T070 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/mod.rs:76`.
- Dependencies: story entry gate.

Claim: Three timers in the frame loop are frame counts with a 60 fps assumption baked into a comment rather than the code: the API health ping at 3600 frames ("~every 60s at 60fps", line 74), the save-status auto-dismiss at 180 frames ("~180 frames (~3s at 60fps)", line 85-88), and the tab-alert pulse rate at line 467 ("~3s breathe at 60fps"). The same function knows the right pattern — the chat timeout right below it is explicitly `Duration`-based with the comment "Wall clock, not FPS" (line 94).

Remediation decision: Use `Instant`/`Duration` the way the chat timeout immediately below already does: store `last_api_check: Option<Instant>` and `save_status_at: Option<Instant>`, and drive the pulse from elapsed seconds rather than `frame_count`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W063

- Task: T071 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/mod.rs:288`.
- Dependencies: story entry gate.

Claim: This status-bar branch is unreachable and the field behind it is write-only. `grep -rn 'names_loading|names_stage|names_lang' crates` returns every use: declared at state.rs:528-530; `names_loading` is assigned `false` at stats.rs:114 and stats.rs:167 and never `true` anywhere; `names_stage` is only ever `.clear()`ed (stats.rs:115, :168) and never assigned a message, so even if the branch ran it would render `"| "`; `names_lang` is written at stats.rs:163 and never read.

Remediation decision: Either set `names_loading = true` plus a `names_stage` message before the `spawn_worker` at stats.rs:146 (and handle the `!spawned` case like every other site in the file), or delete the three fields and this render branch.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W064

- Task: T072 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/mod.rs:942`.
- Dependencies: story entry gate.

Claim: `render_radar_chart` is documented "Returns true if weights were modified by user interaction" (crates/addon/src/ui/radar_chart.rs:57) and computes that flag through its whole body, but `grep -rn render_radar_chart crates` shows exactly one caller — this one — and it binds the result to a discarded `_chart_modified`. The function's return value is dead across the whole repo.

Remediation decision: If weight edits should invalidate results, use the flag the way the role/mode/scale handlers do; otherwise drop the binding and change the signature to `-> ()`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W065

- Task: T073 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:108`.
- Dependencies: B001.

Claim: The optimizer-StatBlock -> core-StatBlock conversion (13 fields, 9 of them `.round() as i32`) is written out inline four times in this batch: optimization.rs:108-122, optimization.rs:408-422, optimization.rs:1064-1078, and once as a named helper `opt_stats_to_stat_block` in resolution.rs:254-273 — the same module tree, private only by default. grep for `crit_damage: derived.crit_damage` / `candidate.derived.crit_damage` returns exactly those four sites.

Remediation decision: Make `resolution::opt_stats_to_stat_block` `pub(super)` (or move it to a shared `stats` helper alongside `perf_to_combat_metrics`) and call it from all three optimization.rs sites.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W066

- Task: T074 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:212`.
- Dependencies: story entry gate.

Claim: These two literals silently duplicate `gw2_optimizer::scoring::STRIKE_DPS_NORM = 3000.0` and `CONDI_DPS_NORM = 3500.0` (crates/optimizer/src/scoring.rs:17-18), which are `pub const` and importable from here. The next line (213) does the same with `3500.0`.

Remediation decision: `use gw2_optimizer::scoring::{STRIKE_DPS_NORM, CONDI_DPS_NORM};` and divide by those.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W067

- Task: T075 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:491`.
- Dependencies: B001.

Claim: The `empty_kit` parameter has exactly one production caller and it passes the literal `true` (line 483, `data_quality: leftover_plate_quality(true)`). `grep -rn leftover_plate_quality crates` shows the only `false` argument is in a test (line 2279). The `else` branch returning `DataQuality::Verified` is therefore unreachable in the shipped DLL, and the function is a constant dressed as a decision.

Remediation decision: Drop the parameter and return `DataQuality::Blocked` (keeping the doc comment as the justification), or compute `empty_kit` from the candidate at the call site so the branch becomes real.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W068

- Task: T076 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:626`.
- Dependencies: W035.

Claim: The comment admits the coupling: this is a hand-copied duplicate of `REFERENCE_WEAPON_STRENGTH: f64 = 1100.0` (crates/optimizer/src/combat.rs:412), which is a *private* const and so cannot be imported. `grep -rn 1100.0 crates` shows the same literal re-typed again at optimizer/engine.rs:1296 and gemini_tools.rs:1527/1529.

Remediation decision: Make `REFERENCE_WEAPON_STRENGTH` `pub` in `gw2_optimizer::combat` and use it here (and at engine.rs:1296 / gemini_tools.rs:1527).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W069

- Task: T077 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimization.rs:1585`.
- Dependencies: story entry gate.

Claim: This doc comment, the two `//` lines under it and the `#[allow(clippy::too_many_arguments)]` at line 1589 are left over from a function that moved: they now sit directly on `#[cfg(test)] mod tests` (1590-1591). The identical four-line block still correctly documents `enrich_with_llm` at optimize_flow.rs:634-638. So optimization.rs's test module is documented as "Call the active LLM provider…" and carries a clippy suppression for arguments it does not have.

Remediation decision: Delete lines 1585-1589; `#[cfg(test)] mod tests` needs no doc comment and no clippy allow.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W070

- Task: T078 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/optimize_flow.rs:229`.
- Dependencies: W123.

Claim: Both LLM client constructions inside the optimize worker discard their error with `.ok()` and log nothing (line 229 for the optimize_v2 advisor pass, line 306 for the deterministic tier). The same call in `enrich_with_llm` (line 652) does `map_err(|e| e.to_string())?` and the failure is reported, so the swallow is inconsistent within one file.

Remediation decision: Bind the result and log the `Err` at `LogLevel::Warning` (the same channel the tier fallbacks below already use) before dropping to `None`; consider pushing a `DataQualityReason` so the served suggestion says the advisor pass was skipped.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W071

- Task: T079 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/resolution.rs:308`.
- Dependencies: story entry gate.

Claim: `ResolvedSpec::traits_available` is never populated in production. `grep -rn traits_available crates` returns exactly 7 hits: the field declaration (crates/core/src/types.rs:156), this unconditional `Vec::new()` — the only production write, in the only resolver that builds a `ResolvedBuild` from a character — two test fixtures (optimization.rs:2147, :2204), one test that does populate it (optimize_flow.rs:1741), and two production readers that iterate it as a fallback: `fill_holes_from_loadout` (optimization.rs:1394) and `selected_trait_names` (optimize_flow.rs:1194).

Remediation decision: Decide and encode it: either populate `traits_available` in `resolve_specs_db` from `spec.major_traits` so the fallbacks are real, or delete the field, both fallback loops, and the test that pretends they run.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W072

- Task: T080 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/resolution.rs:420`.
- Dependencies: story entry gate.

Claim: The four weapon-slot arms of `resolve_equipment_db` (420-475) are the same 13 lines copy-pasted four times, differing only in `ws1`/`ws2` and `main_hand`/`off_hand`: each builds an identical `WeaponInfo { name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.as_deref()).map(canonical_weapon_type).unwrap_or_default(), id }`, then repeats the same `if ws.stat_prefix.is_empty()` guard and the same `extract_sigils` call.

Remediation decision: Match to `(set, is_main)` first — `"WeaponA1" => (&mut ws1, true)`, … — then run the shared body once against that pair.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W073

- Task: T081 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/about.rs:182`.
- Dependencies: story entry gate.

Claim: about.rs carries verbatim private copies of two saveload.rs helpers, each annotated as such: `format_sent` (about.rs:184-236) duplicates `format_timestamp` (saveload.rs:1039-1092, ~50 lines of hand-rolled leap-year/civil-date arithmetic), and `paint_row_plate` (about.rs:418-431) duplicates saveload.rs:532-545 byte-for-byte. grep `fn format_timestamp|fn format_sent|fn paint_row_plate` across crates/ returns only these four sites. The crate already depends on chrono (crates/addon/Cargo.toml:23, used at settings.rs:1642 and news.rs:825-837), so the date formatter is a reinvention as well as a duplicate.

Remediation decision: Move `format_timestamp` and `paint_row_plate` to a `pub(super)` helper module under main_view (or theme.rs for the plate) and delete the copies; replace the date arithmetic with `chrono::DateTime::<Utc>::from_timestamp(ts, 0).format("%Y-%m-%d %H:%M")`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W074

- Task: T082 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/about.rs:433`.
- Dependencies: story entry gate.

Claim: Width-aware ellipsis truncation is implemented five times in the addon: about.rs:434 `clip_label` (self-described copy), saveload.rs:161 `clip_label`, ui/news_feed.rs:406 `clip_label`, ui/theme.rs:485 `clip_to_width` (private), radio.rs:1635 `clip_text`. grep `fn clip_label|fn clip_text|fn clip_to_width` across crates/ returns exactly these five. Four of them are the O(n²) variant (clone the growing prefix and re-measure it per char); radio.rs:1642-1644 already contains the O(n) rewrite with a comment explaining why it matters per row per frame. about.rs calls its O(n²) copy three times per message row per frame (703, 705, 717).

Remediation decision: Make `theme::clip_to_width` `pub(crate)`, replace its body with the O(n) accumulate-advances version from radio.rs:1635, and delete the four local copies.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W075

- Task: T083 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/about.rs:472`.
- Dependencies: story entry gate.

Claim: `wrap_pos_local` exists twice with identical body AND an identical unit test: about.rs:472-474 (`pub(super)`, test at 1036) and ui/theme.rs:735-737 (`pub(crate)`, test at 1776). grep `fn wrap_pos_local` across crates/ returns both. On top of that, `wrapped_to` (text_colored pinned to a screen-space right edge) is implemented identically in about.rs:486-491 and wizard.rs:888-893, the latter calling `super::wrap_pos_local` instead of the already-`pub(crate)` theme one.

Remediation decision: Delete about.rs `wrap_pos_local` and its test, point both `wrapped_to` copies at `theme::wrap_pos_local`, and promote one `wrapped_to` into theme.rs next to `theme::wrapped`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W076

- Task: T084 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/about/wizard.rs:579`.
- Dependencies: story entry gate.

Claim: `draw_fmt_icon` has a 17-line `FmtIcon::Bold` arm (wizard.rs:579-595) that draws a vector 'B' from a stroke and two circles, but its only caller (wizard.rs:554, confirmed by grep `draw_fmt_icon` across crates/: one definition, one call) sits in the `else` branch of `if icon == FmtIcon::Bold` (546), which renders the bold glyph as text instead. The arm can never execute.

Remediation decision: Delete the `FmtIcon::Bold` arm in `draw_fmt_icon` (or move the text-'B' rendering into it and drop the special case at line 546 so one path owns the icon).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W077

- Task: T085 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/improve.rs:84`.
- Dependencies: story entry gate.

Claim: The free-text `BuildSuggestion.label` doubles as a type tag: optimization.rs:462 formats it as `format!("Score: {:.2}", candidate.score)`, and two renderers sniff it back with `starts_with("Score:")` (improve.rs:84, ui/comparison.rs:462) to decide the tab caption. grep `"Score:` across crates/ returns exactly these three sites. The same file also routes `ImproveOutcome::from_label(&..label)` (improve.rs:118), so the label string is parsed by two independent protocols.

Remediation decision: Add an explicit `label_kind: LabelKind` (or `Option<f64> score`) field on `BuildSuggestion` set at optimization.rs:462, and branch on that in improve.rs and comparison.rs; if the string protocol must stay, name the prefix once as a `pub const`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W078

- Task: T086 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/news.rs:37`.
- Dependencies: story entry gate.

Claim: `render_news_tab` calls `state.news.collected(&sources)` three times per frame — line 26-28 (to build `items`), line 37 (only to test emptiness), and again inside `masthead` at lines 72-75 (only for `.len()`). `NewsState::collected` (crates/addon/src/news.rs:94-101) clones every `NewsItem` (title, summary, urls, image url) from every enabled source into a fresh Vec and sorts it. The count and the emptiness test are both derivable from the `items` Vec already built at line 26 (before the filter) or from the first call's length.

Remediation decision: Call `collected` once, keep `let total = all.len()` before filtering, pass `total` into `masthead`, and derive `empty` from `total == 0`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W079

- Task: T087 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/radio.rs:443`.
- Dependencies: story entry gate.

Claim: The radio-search worker wraps the directory call in `catch_unwind` (radio.rs:439-443) and maps a panic to the plain string "station search failed", which is then stored in `radio.last_error` and rendered as an ordinary search error; the panic payload is dropped and nothing is written to the Nexus log (grep `set_hook` in crates/addon/src: none, so the default stderr hook reaches nobody in the game process). settings.rs:1622-1625 does the same for the benchmark sync (`benchmark_error = Some("thread panicked")`) without logging, whereas the neighbouring `spawn_key_validation` at settings.rs:199-204 does log the panic. Note `spawn_worker` (state.rs:417-421) only logs when the panic escapes the closure, which these inner catches prevent.

Remediation decision: In both catch arms, extract the payload (`e.downcast_ref::<&str>()`/`String`) and `nexus::log::log(LogLevel::Critical, ..)` it before setting the user-facing message; better, route both through `spawn_flag_guarded` and log once in its `apply` on `Err(_)`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W080

- Task: T088 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/saveload.rs:69`.
- Dependencies: story entry gate.

Claim: `render_save_build_ui` (saveload.rs:36-82) and `corral_current` (307-351) are the same save flow written twice: resolve character/profession/game_mode, build BalanceContext, `suggestion_to_saved`, `BuildStorage::save_new`, then set the three-field status. The status triple (`save_status`/`save_status_err`/`save_status_frames`) is assigned by hand in seven places (71-73, 78-80, 339-341, 346-348, 364-366, 399-401, 404-406) and the success block additionally repeats `save_name_input.clear()` + `saved_builds_loaded = false`. The only real difference is `corral_current` preferring `selected_character_name` over the build's character name.

Remediation decision: Collapse both into one `save_current_suggestion(state, character_name)` and one `set_save_status(state, msg, is_err)`; call them from the New Build/Improve footer and the ranch corral bar.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W081

- Task: T089 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/saveload.rs:193`.
- Dependencies: story entry gate.

Claim: The 'inert button at 40% alpha with a why-tooltip' idiom is implemented as three near-identical named helpers — saveload.rs:193 `muted_gold`, wizard.rs:896 `dimmed_gold`, about.rs:462 `dimmed_warn` — and inlined by hand at least seven more times: settings.rs:278-280, 318-320, 1418-1420, 1468-1470, 1573-1582, saveload.rs:27-33, wizard.rs:1254-1257. All push `StyleVar::Alpha(0.4)`, draw the button, pop, and (mostly) tooltip on hover. grep `Alpha(0.4)` in crates/addon/src/ui/main_view/tabs confirms 10 sites.

Remediation decision: Add `theme::dimmed_button(ui, label, size, tip: Option<&str>)` (gold variant + a colour parameter for the amber Resend button) and replace all ten sites.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W083

- Task: T090 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/saveload.rs:832`.
- Dependencies: story entry gate.

Claim: `saved_to_suggestion` carries two save-format migration shims in the live load path: (a) lines 832-842 substitute "Warrior" for an empty `profession` and then compute `compute_derived`/`compute_3tier_combat` with Warrior's HP/armor class for whatever class the save really was (only a Warning log); (b) `gear_prefixes: GearPrefixGroups` is written as `default()` by every writer (saveload.rs:808, core storage.rs:384 — grep `gear_prefixes` across crates/ shows no non-default writer) yet still read via `GearSlots::from_legacy(&saved.stat_prefix, &saved.gear_prefixes)` at 879-881, so the field exists only to deserialize old files.

Remediation decision: Normalize legacy saved-build fields on load with explicit missing-profession handling; never silently assume Warrior or remove supported read compatibility.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W084

- Task: T091 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/settings.rs:300`.
- Dependencies: story entry gate.

Claim: Untranslated English strings reach the player in an overlay whose chrome is localized in 12 languages (CLAUDE.md: 'Overlay chrome is localized'; settings.rs:1846 even has a test asserting theme keys exist). Sites: settings.rs:300 Hide/Show reveal button; settings.rs:210 and 1623 status text "thread panicked"; settings.rs:738 `.unwrap_or("English")`; saveload.rs:506 comparison.error "Could not start the load thread - the system refused it. Try again."; saveload.rs:758 "{} corrupt save file(s) skipped: {}" in red on the Saves tab; radio.rs:443 "station search failed" shown by `theme::wrapped(ui, theme::ERR, err)` at 576; kitchen.rs:12-30 STARTERS pair a localized pill label with an English prompt that is queued as the player's own chat bubble. Every neighbouring string in these files goes through `t()`/`tf()`.

Remediation decision: Add locale keys (`btn.show_key`, `btn.hide_key`, `err.thread_panicked`, `err.spawn_refused`, `ranch.corrupt_skipped`, `radio.error.search_failed`, `starter.*.prompt`) and route each site through `t()`/`tf()`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W085

- Task: T092 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/settings.rs:520`.
- Dependencies: story entry gate.

Claim: `render_model_picker_section` re-implements two helpers that already exist: lines 520-525 match on `active_provider` to pick the model id, which is exactly `AppConfig::active_model_id()` (crates/core/src/config.rs:745-752, and used by the sibling `render_talk_model_row` at settings.rs:443); lines 536-549 re-implement the hardcoded-catalog fallback that `model_catalog()` at settings.rs:369-383 already provides in the same file (also used by the sibling at 444). grep `fn model_catalog` shows one definition and one caller — the picker section never calls it.

Remediation decision: In `render_model_picker_section` replace lines 520-525 with `state.config.active_model_id().to_string()` and lines 536-549 with `model_catalog(state)`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W086

- Task: T093 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/main_view/tabs/settings.rs:595`.
- Dependencies: story entry gate.

Claim: The per-provider usage-counter filenames are string literals in two crates with no shared constant: settings.rs:594-599 (the reader that displays 'requests today') and crates/optimizer/src/llm/mod.rs:242/252/262/272 (the writer). grep `_usage\.json` across crates/ returns exactly those eight literals and nothing else. The reader also re-parses the JSON key `"requests_today"` (settings.rs:608) as a free-form serde_json::Value lookup rather than through the writer's type.

Remediation decision: Expose `LlmProvider::usage_file_name(&self) -> &'static str` (or a `pub fn usage_path(addon_dir, provider)`) in gw2_core::config or gw2_optimizer::llm and a typed `read_usage()` returning the count; use it from both the writer and settings.rs.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W087

- Task: T094 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/news_feed.rs:406`.
- Dependencies: W074.

Claim: Three private implementations of "measure text, truncate char-by-char, append an ellipsis" exist across this batch: `news_feed::clip_label` (406-422), `theme::clip_to_width` (485-504) and `comparison::truncate_ui_text` (803-820). The first two are near-identical (theme's only extra is a `max_w <= 4.0` guard); the third differs only in budgeting the ellipsis width up front instead of probing with it appended.

Remediation decision: Consolidate with W074; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W088

- Task: T095 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/radar_chart.rs:286`.
- Dependencies: story entry gate.

Claim: `render_presets` has no caller anywhere. `grep -rn --include=*.rs 'render_presets' crates server` returns exactly one hit: this definition (the `.symforge/` tee copies excluded). It is `pub` in a `pub mod` of a cdylib lib crate, so rustc's dead_code lint cannot see it. It is the sole consumer of `OptimizationWeights::PRESETS` outside that type's own tests: `grep -rn 'PRESETS' crates` gives only this line, scoring.rs:370 (the definition) and scoring.rs:1136/1562 (tests).

Remediation decision: Delete `render_presets`; then `OptimizationWeights::PRESETS` and its six preset fns have no non-test consumer and can follow in the optimizer crate.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W089

- Task: T096 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/setup.rs:556`.
- Dependencies: story entry gate.

Claim: The initial download progress state hardcodes `total_steps: 14`, which is the sum of two *private* constants in another crate: `crates/gw2api/src/download.rs:22` `const TOTAL_STEPS: usize = 10` plus `download.rs:283` `const NAME_STEPS: usize = 4`. Neither is `pub`, so the addon cannot reference them and nothing links the literal to its source.

Remediation decision: Make `download::TOTAL_STEPS + NAME_STEPS` public as a single `pub const DOWNLOAD_STEPS: usize` in gw2api and use it here.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W090

- Task: T097 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/theme.rs:544`.
- Dependencies: story entry gate.

Claim: The selected/hovered/idle fill+rim+text palette decision is written out three times verbatim in the same file: `pill_pulse` (544-557), `select_chip` (612-626) and `segment_row` (701-715). Each is then followed by the same filled-rect-plus-outline draw pair with the same `rounding(h * 0.45)`.

Remediation decision: Add one `fn chip_colors(p: &Palette, selected: bool, hovered: bool) -> (fill, rim, text)` and one `fn draw_pill_rect(dl, p, w, h, fill, rim)`, and have all three call sites use them.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W091

- Task: T098 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/addon/src/ui/theme.rs:924`.
- Dependencies: story entry gate.

Claim: The first-run data-download screen renders hardcoded English strings: this NOTE, the heading `"FETCHING TYRIA..."` (line 952) and the checkpoint labels `["Bronze", "Silver", "Gold", "GEM"]` (line 946). The same pattern recurs in `comparison::viability_gate_label` (comparison.rs:1111, eleven English gate names) and `chat_links` ("Build template" at lines 103 and 257). None of these route through `gw2_core::i18n::t`.

Remediation decision: Add `setup.fetching`, `setup.speed_note`, `setup.tier_bronze/silver/gold/gem` (and `viability.gate.*`) to locales/*.json and call `t()` at these sites, as the rest of the wizard already does.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W092

- Task: T099 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/core/src/feedback/report.rs:10`.
- Dependencies: story entry gate.

Claim: The feedback wire limits are declared twice, once per deployable, with no shared crate and nothing checking they agree. `crates/core/src/feedback/report.rs:10,12,14` (`MAX_BODY_CHARS = 4000`, `MAX_TITLE_CHARS = 120`, `MAX_SNAPSHOT_BYTES = 6 * 1024`) are verbatim duplicates of `server/feedback/src/reports.rs:15,16,17`, and `MAX_REQUEST_BYTES = 16_000` (report.rs:16) mirrors `server/feedback/src/app.rs:16: MAX_REQUEST_BYTES = 16 * 1024` with a comment claiming headroom. They currently agree - I compared both sides - so this is drift risk, not a live bug.

Remediation decision: Either publish the limits from one place (a tiny shared crate, or have the client read them from the `/v1/taxonomy` response it already fetches), or add a test that fails when the two constant sets diverge.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W093

- Task: T100 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/core/src/storage.rs:135`.
- Dependencies: story entry gate.

Claim: `BuildStorage::save(build, overwrite)` is a public convenience wrapper with no production caller. Every real save site calls the underlying methods directly: `crates/addon/src/ui/main_view/tabs/saveload.rs` lines 69 and 337 use `save_new`, lines 127, 394 and 464 use `save_overwrite`. `grep -rn --include=*.rs 'storage.save(' crates server` returns only storage.rs:911/915/918, all inside `test_save_convenience_method`.

Remediation decision: Delete `save` and `test_save_convenience_method`; callers already use the explicit `save_new` / `save_overwrite` pair.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W094

- Task: T101 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/core/src/storage.rs:184`.
- Dependencies: story entry gate.

Claim: Three `eprintln!` calls survive in code that ships inside the injected DLL, where there is no console and no one reads stderr. This one is in `BuildStorage::list_with_skipped`; the other two are in `FeedbackStore::load` and `FeedbackStore::load_taxonomy`. The codebase already knows this: storage.rs:155 says corrupt files "used to be dropped with only an `eprintln!` that an injected DLL's caller never sees", and the addon has a real channel (`crate::ui::log_disk_error`, used at `crates/addon/src/feedback/tasks.rs:698`). `grep -rn 'eprintln!' crates/core/src crates/gw2api/src` returns only these three plus the dev-only `dev_config.rs:50`.

Remediation decision: Drop the prints (the skipped-name list and the returned error already carry the information to callers), or return the diagnostic so the addon can route it to the Nexus log / status bar.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W095

- Task: T102 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/core/src/types.rs:99`.
- Dependencies: story entry gate.

Claim: `BuildLocks::describe_constraints` has no production caller - `grep -rn --include=*.rs 'describe_constraints' crates server` returns only this definition plus eight snapshot tests inside the same file's `#[cfg(test)]` module. The test module's own header comment (types.rs:653-658) states it: "Its former consumer, `crates/optimizer/src/prompts.rs::synergy_build_prompt`, was dead code (zero production callers) and was deleted; `describe_constraints` currently has no production caller either." The LLM-facing formatter that IS live is the optimizer's `describe_lock_constraints`.

Remediation decision: Delete `describe_constraints` and its eight snapshot tests, or wire the live prompt builder to it so the format has exactly one owner.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W096

- Task: T103 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/cache.rs:44`.
- Dependencies: story entry gate.

Claim: The temp-write + flush + rename + orphan-cleanup block is copy-pasted three times in this file with only the serialized value changing: `save` (cache.rs:44-54), `save_character` (cache.rs:185-195) and `save_characters` (cache.rs:221-231) are otherwise byte-identical immediately-invoked closures, each followed by the same `if result.is_err() { let _ = std::fs::remove_file(&tmp_path); }`.

Remediation decision: Extract one private `fn write_json_atomic<T: Serialize>(&self, key: &str, value: &T) -> Result<(), CacheError>` and have all three call it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W097

- Task: T104 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/cache.rs:118`.
- Dependencies: story entry gate.

Claim: `DataCache::delete` has zero callers. `grep -rn --include=*.rs '\bdelete(' crates server` returns only this definition plus `BuildStorage::delete` (a different type, called from `saveload.rs:518`) and that type's tests. Cache invalidation in production goes through `clear_all` (`crates/addon/src/ui/main_view/tabs/settings.rs:1423,1473`) or through `is_stale` + overwrite, never per-key deletion. It also has no test of its own.

Remediation decision: Delete the method.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W098

- Task: T105 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/client.rs:490`.
- Dependencies: story entry gate.

Claim: Every request to the GW2 API identifies the addon as version 0.1. The workspace version is 1.11.26 (`Cargo.toml: version = "1.11.26"` under `[workspace.package]`), and the string is a `from_static` literal, so it has never tracked the crate version - `env!("CARGO_PKG_VERSION")` is available and unused here.

Remediation decision: Build the header once from the crate version, e.g. `HeaderValue::from_static(concat!("GW2BuildOptimizer/", env!("CARGO_PKG_VERSION")))`, and hoist it out of the per-attempt loop.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W099

- Task: T106 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/client.rs:767`.
- Dependencies: story entry gate.

Claim: `Gw2Client::validate_api_key` has zero callers anywhere - `grep -rn --include=*.rs 'validate_api_key' crates server` returns only this definition (client.rs:767-768, 778); there is no test and no production call. `ApiError::MissingScopes` (client.rs:280) is likewise constructed only inside this dead function and matched nowhere. The live key-validation path, `crates/addon/src/ui/setup.rs:201-226`, re-implements it inline: it does its own `client.get("tokeninfo")` and repeats the same `let required = ["account", "characters", "builds"];` list and the same `missing` filter.

Remediation decision: Either delete `validate_api_key` and `ApiError::MissingScopes`, or make `setup.rs` call it (returning the recommended-scope table it also needs) so the required-scope list exists once.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W100

- Task: T107 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/download.rs:46`.
- Dependencies: story entry gate.

Claim: `propagate_icon_step` is an identity function with a four-line doc comment describing error-classification behaviour it does not implement. Its single production use (download.rs:251) wraps `graphics::download_missing(...)` and is immediately followed by `?`, so removing the call changes nothing. A test, `propagate_icon_step_does_not_swallow_cache` (download.rs:402-416), asserts that the identity function returns its argument for four error variants, plus a second assertion at download.rs:429.

Remediation decision: Delete the function, its call site wrapper (leave the `?` on `download_missing`), and the two tests; move the explanatory comment onto the `download_missing` call if it is worth keeping.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W101

- Task: T108 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/gw2api/src/localize.rs:111`.
- Dependencies: story entry gate.

Claim: The set of official `/v2?lang=` locales is written out four times. This guard duplicates `API_LANGS` declared 58 lines above it in the same file (localize.rs:53: `pub const API_LANGS: &[&str] = &["de", "es", "fr", "zh"];`), which `pack_status` (localize.rs:66) and `download_game_and_names` (download.rs:296) do use. The same four codes are hardcoded again in `Gw2Client::with_lang` (client.rs:360) and a third time as match arms in `gw2_core::i18n::api_lang` (i18n.rs:142-146).

Remediation decision: Make `API_LANGS` the single source: `if !API_LANGS.contains(&lang)` here, `API_LANGS.contains(&c)` in `with_lang`, and have `i18n::api_lang` consult the same list (or move the list into `gw2-core` since `gw2-api` already depends on it).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W102

- Task: T109 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/combat.rs:226`.
- Dependencies: story entry gate.

Claim: Two hardcoded `None` fallbacks that cannot be reached. `condition_weights_for_profession` (L226-236) and `buff_profiles_for_profession` (L553-580, plus the truncate/pad guard at L570-578) both branch on `rotation_profiles().lookup(...)` returning None. `lookup`'s final step (data/rotation_profiles.rs:234) is `profiles.iter().find(|p| p.profession == "Generic")`, and `validate_profiles` (data/rotation_profiles.rs:277-284) rejects any mode whose JSON lacks a "Generic" profile with "missing Generic fallback profile"; `rotation_profiles()` (data/rotation_profiles.rs:29-32) `.expect()`s a successful load of the compile-time `include_str!` JSON. So for all three modes the lookup is infallible in any binary that started. The comments themselves say "Should never happen".

Remediation decision: Use the canonical Generic profile for missing scenario fallback and preserve explicit quality degradation; remove duplicated literal profile values.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W103

- Task: T110 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/combat.rs:769`.
- Dependencies: story entry gate.

Claim: `extract_modifier_from_fact` has a `Fact::Buff` arm that binds `text` and `status` and immediately discards both with `let _ = (text, status);`. It is behaviourally identical to the `_ => {}` catch-all on the very next line, so the arm changes nothing except suppressing the unused-binding warnings its own pattern creates.

Remediation decision: Delete the whole arm (the `_ => {}` below already covers it), or implement the buff handling. If it is a deliberate placeholder for planned work, say so in the comment and reference the story.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W104

- Task: T111 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/combat.rs:1324`.
- Dependencies: story entry gate.

Claim: `apply_upgrade_prose` (L1311-1347) invents damage coefficients for numberless tooltip prose with bare literals and no source, name or data-file entry: 0.07 strike here, 0.10 condition duration (L1329), 0.10 healing (L1333), 7.0 crit chance points (L1340), and 0.05 strike + 0.05 condition (L1343-1344). `upgrade_uptime` (L1272-1309) does the same for its whole cadence table — 40.0 s elite, 8.0 dodge, 9.0 weapon swap, 15.0 foe-CC, 5.0 weapon skill, 4.0 critical, 8.0 default — alongside `UPGRADE_BUFF_S = 6.0` (L1184) and the 1.0-30.0 clamp at L1264. `parse_sigil_modifier` (L1377-1382) hardcodes per-item values keyed off English names: 0.03/0.05 for "sigil of force", 0.04/0.06 for "sigil of bursting".

Remediation decision: Move the prose-fallback coefficients, the trigger-cadence table, UPGRADE_BUFF_S and the per-sigil overrides into a data/ JSON with the same EvidenceLevel/sources treatment as the other formula files, or at minimum promote each to a named `const` with a doc comment stating it is an unsourced heuristic and what it was calibrated against.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W105

- Task: T112 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/context.rs:416`.
- Dependencies: story entry gate.

Claim: section_current_build re-implements prompts::sanitize_build_summary byte-for-byte (same .chars().take(2000).filter(|c| *c != '`' && *c != '<' && *c != '>')) while the comment claims it reuses it. prompts.rs:385 declares `pub(crate) fn sanitize_build_summary`, so context.rs (same crate) can call it directly, and gemini_tools.rs:1004 already does exactly that. gemini_tools.rs has a whole test (get_current_build_is_sanitized, line 2573) whose stated point is that the tool and the prompt must produce byte-identical sanitized text — an invariant this third copy silently sits outside of.

Remediation decision: Replace the inline sanitizer with `crate::prompts::sanitize_build_summary(summary)` and drop the comment.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W106

- Task: T113 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/balance_overrides.rs:327`.
- Dependencies: W014.

Claim: `check_wvw_quality` calls `overrides.lookup(patch_id, "WvW", split.source_type, 0, split.field)` with a literal `0` for `source_id`. `BalanceOverrides::lookup` (line 117) matches on `e.source_type == source_type && e.source_id == source_id`, and no override entity can have GW2 API id 0 — so this lookup is structurally incapable of returning `Some`, and the `Some(OverrideResult::Value)` and `Some(OverrideResult::Unknown)` arms at lines 331 and 336 are unreachable regardless of the data. The surrounding comment block (lines 314-322) acknowledges this and defers ID mapping to "Future P3-13 work".

Remediation decision: Consolidate with W014; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W107

- Task: T114 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/boon_condition_formulas.rs:227`.
- Dependencies: story entry gate.

Claim: Five metadata fields are deserialized from `data/formulas/{boons,conditions}.json`, copied into the resolved structs, and read by nobody. Repo-wide `grep -rn --include=*.rs '<name>' crates server` for `max_duration`, `special_mechanics`, `secondary_effects`, `counterpart_condition` and `suppression_effects` returns hits only inside this file — the raw declaration, the resolved declaration, and the load-time copy (lines 585-592 for boons, 664-666 for conditions). Zero reads anywhere, including tests. The `SuppressionEffects` struct (line 384) and both its fields exist only to type one of them.

Remediation decision: Document provenance-only formula fields; consume genuinely behavioral suppression fields or reject unsupported controls at load, rather than silently advertising them.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W108

- Task: T115 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/mod.rs:69`.
- Dependencies: story entry gate.

Claim: Two enum variants are never constructed anywhere in the repo. `grep -rn --include=*.rs 'MissingRequired' crates server` returns only this declaration and its `Display` arm (mod.rs:88) — the `try_load!` macro (lines 99-117) only ever produces `ParseError` and `ValidationError`. `DataState::Degraded` (line 126) is likewise never built: `initialize()` returns only `Ready` or `Disabled`, and its own doc admits it ("Currently all Phase A loaders are required, so any failure is `Disabled`. Future phases (P3-08+) may introduce optional loaders that produce `Degraded` instead").

Remediation decision: Align data state variants with the real runtime initialization outcomes from W016.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W109

- Task: T116 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/normalized_effects.rs:649`.
- Dependencies: W017.

Claim: In `effect_uptime`, the `Estimated` arm (lines 649-657) and the `Derived` arm (lines 658-670) are semantically identical: both do `effect.uptime_model.uptime.as_ref().and_then(|fv| match fv { FactualValue::Resolved(v) => Some(*v), _ => None }).unwrap_or(0.5)`. Only formatting differs (the Derived arm adds a block and a comment). The comment on the Derived arm calls 0.5 a "placeholder".

Remediation decision: Consolidate with W017; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W110

- Task: T117 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/objective_profiles.rs:120`.
- Dependencies: story entry gate.

Claim: The doc comment lists six valid `interaction_priorities` keys including `steals_boon`, but `validate_profile`'s `valid_interaction_keys` array (lines 340-346) contains only five and omits `steals_boon`. There is also no `StealsBoon` variant in `normalized_effects::OperationType` (lines 205-214, seven variants, none named Steals). The `#[cfg(test)]` mirror lists at consistency_tests.rs:734 and :814 also omit it, confirming the doc — not the validator — is the outlier.

Remediation decision: Delete `steals_boon,` from the doc comment at line 120 (it is not a modeled operation), or add both the `valid_interaction_keys` entry and an `OperationType::StealsBoon` variant if boon-stealing is meant to be scored.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W111

- Task: T118 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/quality.rs:80`.
- Dependencies: story entry gate.

Claim: `FactualValue`'s convenience and arithmetic API is entirely unexercised outside its own unit tests. `grep -rn --include=*.rs 'is_resolved\|is_unknown\|map_or_unknown' crates server` returns hits only in quality.rs, and those are the definitions (lines 80, 85, 107) plus the tests at lines 301-336. The eight operator impls (`Mul`/`Add`/`Sub`/`Div` for `FactualValue<f64>` against both `f64` and `FactualValue<f64>`, lines 145-228) have no call site anywhere: every real consumer in normalized_effects.rs pattern-matches `FactualValue::Resolved(v) => v` by hand instead (e.g. lines 523-526, 651-657, 662-668).

Remediation decision: Remove unused arithmetic/convenience operators after checking production and integration-test callers; preserve live FactualValue semantics.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W112

- Task: T119 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/rotation_profiles.rs:267`.
- Dependencies: story entry gate.

Claim: A public loader whose doc says "Exposed for testing" and whose only callers are tests. `grep -rn --include=*.rs 'load_rotation_profiles' crates server` returns this definition, `try_load_rotation_profiles` (a different function) at data/mod.rs:161, and nothing else outside this file's `#[cfg(test)] mod tests`. It is not used by the real loader either: `load_all_rotation_profiles` (line 254) independently does its own `serde_json::from_str` + `validate_profiles` for each of the three modes, so the parse/validate sequence exists twice.

Remediation decision: Make it `#[cfg(test)] fn`, or have `load_all_rotation_profiles` call it per-file with the real mode label so there is one parse/validate implementation and the tests exercise the production path.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W113

- Task: T120 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/rotation_profiles.rs:408`.
- Dependencies: story entry gate.

Claim: This call site hardcodes the verb-form key "Poison" because `data/rotation_profiles/*.json` uses verb-form keys while every other dataset in this crate uses the canonical status-effect form. Verified: `grep -o '"Poison[a-z]*"' data/rotation_profiles/pve.json` -> 10x "Poison"; the same grep on `data/formulas/conditions.json` -> 1x "Poisoned". `condition_weight` (line 145) documents the divergence as a deliberate deferral ("switching to `canonical_condition_name` would require migrating the JSON keys ... out of scope") and deliberately does NOT normalize, unlike `ConditionFormulas::tick_damage` (boon_condition_formulas.rs:441) and `objective_profiles::load_objective_profile_file` (objective_profiles.rs:209), which both normalize on the way in.

Remediation decision: Migrate the `condition_application` keys in the three rotation_profiles JSON files to canonical form and route `condition_weight` through `canonical_condition_name` like the sibling loaders; failing that, add a load-time validation that every `condition_application` key resolves via `canonical_condition_name` to a key present in `formulas/conditions.json`, so a rename fails loudly instead of silently scoring 0.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W114

- Task: T121 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/rotation_profiles.rs:416`.
- Dependencies: story entry gate.

Claim: `default_pve`, `necro_group` (line 425) and `firebrand_group` (line 434) are reachable only from test code, and the doc admits it ("Used in tests and as a compatibility bridge"). `grep -rn 'default_pve\|necro_group\|firebrand_group' crates server` gives hits at combat.rs:1599-2828 and scoring.rs:1361-1440 only — combat.rs's `#[cfg(test)]` modules start at lines 995 and 1405, and scoring.rs's at line 987, so every one of those call sites is inside a test module. The remaining hits are this file's own tests (416/425/434 region and 419/428/437).

Remediation decision: Move test-only convenience constructors behind test support while keeping production from_profile.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W115

- Task: T122 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/slot_budgets.rs:209`.
- Dependencies: story entry gate.

Claim: Two doc comments in this file orient the reader against code that no longer exists anywhere in the repo. `grep -rn --include=*.rs 'attribute_adjustment_for_slot' crates server` -> zero hits; `grep -rn --include=*.rs 'SLOT_ADJUSTMENTS' crates server` -> zero hits. Line 209 says `major_for_api_slot` "replaces the old `attribute_adjustment_for_slot()` function"; line 169 says `EQUIPMENT_SLOTS` "Matches the layout of the old `SLOT_ADJUSTMENTS` constant".

Remediation decision: Replace both references with a self-contained statement of the invariant (e.g. "16 entries: 6 armor + 4 weapon slots across 2 sets + 6 trinkets; main-hand slots draw the two-hand budget"), and drop the "replaces the old ..." sentence at line 209.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W116

- Task: T123 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/slot_budgets.rs:225`.
- Dependencies: story entry gate.

Claim: Unused accessors across the batch, none with a caller outside its own module's tests. `get_for_attr_count` (slot_budgets.rs:225) has zero references repo-wide — not even a test — despite its 8-line doc describing the 3/4/7+ attribute mapping. Also verified dead by repo-wide grep: `NormalizedEffectsData::effect_count` (normalized_effects.rs:412, tests only), `BalanceOverrides::entity_count` (balance_overrides.rs:150, tests only), `RotationProfileData::total_count` (rotation_profiles.rs:247, zero hits), `RotationProfile::scenario` (rotation_profiles.rs:166, zero hits), `CleanseRegistry::for_profession` (cleanse_sources.rs:228, only cleanse_sources.rs:329 in tests), `BoonFormulas::vulnerability_max_stacks` (boon_condition_formulas.rs:321, only its own test at :863), and four `is_empty()` methods (boon_condition_formulas.rs:339 and :536, slot_budgets.rs:202, profession_profiles.rs:108) with zero call sites anywhere.

Remediation decision: Remove unused budget accessors after current reference checks; preserve the shape-aware path in use.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W117

- Task: T124 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/data/universal_formulas.rs:100`.
- Dependencies: story entry gate.

Claim: `UniversalFormulas::health()` is dead and its formula is copy-pasted at two live call sites instead. `grep -rn --include=*.rs '\.health(' crates server` returns only universal_formulas.rs:272 (its own test). Meanwhile `grep -rn 'vitality_to_health' crates server` shows combat.rs:494 `let health = stats::base_health(profession) + stats.vitality * f.vitality_to_health;` and stats.rs:568 `let health = base_health(profession) + stats.vitality * f.vitality_to_health;` — both inlining exactly what `health()` does. The sibling `strike_damage()` (line 108) is likewise called only from its own test at line 286/295, while combat.rs:457 hand-builds the armor term from `f.tooltip_reference_armor`.

Remediation decision: Use the canonical health formula helper; remove unused strike helper only if the live call shape cannot use it without behavior changes.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W118

- Task: T125 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:434`.
- Dependencies: story entry gate.

Claim: `optimize_cancellable` and `optimize_pvp` each declare a private local struct with the identical five fields and identical types — `PrecomputedSpec` (engine.rs:251-257) and `PvpPrecomputedSpec` (engine.rs:434-440) — and then run near-identical 40-line precompute loops over `spec_combos` (engine.rs:258-297 vs 441-478). Both walk `cores.chain(elite)`, extend `spec.minor_traits`, call `select_best_major_traits` with the same five arguments, call `stats::calculate_trait_stats_for_mode(&trait_ids, traits_cache, &ctx.game_mode)`, and call `combat::extract_damage_modifiers(&trait_ids, None, &[], None, traits_cache, <cache>, ctx)`. The only difference is the items cache argument: `_items_cache` in one, a locally-constructed `empty_items_cache` (engine.rs:433) in the other. The duplication is already known and pinned in place: the test `spec_precompute_passes_game_mode_to_trait_stats` (engine.rs:4299-4320) reads engine.rs with `include_str!` and asserts the string `calculate_trait_stats_for_mode(&trait_ids, traits_cache, &ctx.game_mode)` appears exactly twice.

Remediation decision: Hoist one `struct PrecomputedSpec` to module scope and extract a single `fn precompute_specs(spec_combos, specs_cache, traits_cache, items_cache, stat_weights, locks, ctx) -> Vec<PrecomputedSpec>`, called by both paths with `&db.items` / `&HashMap::new()` respectively. The include_str! assertion at engine.rs:4313-4319 then becomes a normal behavioural test on the single helper.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W119

- Task: T126 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:1273`.
- Dependencies: story entry gate.

Claim: Both batch files fail the repository's own declared formatting gate. `.github/workflows/ci.yml:16` runs `cargo fmt --all --check`, and `rustfmt --edition 2021 --check` reports diffs in engine.rs at lines 1270 (this over-long call needing rewrap), 1319 (a stray second consecutive blank line at engine.rs:1322, which rustfmt's default blank_lines_upper_bound=1 collapses) and 4098, and in validation.rs at line 2274. I confirmed the gate itself is failing, not just my invocation: `cargo fmt --all --check` from the workspace root reports 110 diff hunks, so this is repo-wide rather than specific to these two files — but four of those hunks live in my batch.

Remediation decision: Run `cargo fmt --all` once across the workspace and commit the result as a single formatting-only change (so it does not pollute future git blame on logic edits), then confirm `cargo fmt --all --check` exits clean. If the reformat is judged too disruptive to land wholesale, at minimum remove the stray blank line at engine.rs:1322, which is pure noise.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W120

- Task: T127 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:1292`.
- Dependencies: W035.

Claim: `prepare_validated_rotation` hardcodes two game constants as bare literals even though both already exist as named, single-source values in the same crate. Line 1292 multiplies by `15.0` — the ferocity-per-1%-crit-damage divisor — which the patch-aware formula table owns as `UniversalFormulas::ferocity_per_crit_damage_pct` (crates/optimizer/src/data/universal_formulas.rs:62, validated positive at line 144, shipped as 15 in the embedded JSON at lines 318/344/369). Line 1296 writes `let weapon_strength = 1100.0;`, which is character-for-character `combat::REFERENCE_WEAPON_STRENGTH` (crates/optimizer/src/combat.rs:412, documented there as 'Ascended greatsword average… an empirical reference baseline'). The same 1100.0 literal is re-typed a third time in gemini_tools.rs:1527 and 1529.

Remediation decision: Read the divisor from the formula table: `crate::data::universal_formulas::formulas().ferocity_per_crit_damage_pct` instead of `15.0`. Make `combat::REFERENCE_WEAPON_STRENGTH` `pub(crate)` and use it at engine.rs:1296 (and at gemini_tools.rs:1527/1529) instead of re-typing 1100.0.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W121

- Task: T128 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:1957`.
- Dependencies: story entry gate.

Claim: Two of the three optimizer tier entry points accept parameters they never read, and the callers do real work to supply them. `optimize_deterministic_cancellable` (engine.rs:1951) takes `_current_build_summary: Option<&str>` and never mentions it again in the function body (lines 1962-2137); the addon builds that summary at optimize_flow.rs:99, threads it through `run_deterministic_tier` (optimize_flow.rs:581, 593) and the public wrapper `optimize_deterministic` (engine.rs:1928, 1939) before it is dropped. `optimize_cancellable` (engine.rs:119) takes `_current_equipment: Option<&EquipmentTab>` which is likewise never read, and its only production caller passes a literal `None` (optimize_flow.rs:619). Separately, the neighbouring `_items_cache` (engine.rs:125) IS used — it is passed to `combat::extract_damage_modifiers` at engine.rs:286 — so its underscore prefix asserts the opposite of the truth.

Remediation decision: Drop dead arguments through callers; keep actually-used caches correctly named.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W122

- Task: T129 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:2285`.
- Dependencies: story entry gate.

Claim: Two lint suppressions sit on the wrong items, so they suppress nothing and hide nothing they were written for. engine.rs:2285's `#[allow(clippy::too_many_arguments)]` is anchored on `advisor_rune_pick(db: &GameDb, raw_name: &str)` (engine.rs:2300) — a two-argument function that could never trip the lint; the eight-argument `llm_advisor` it was meant for carries its own correct copy at engine.rs:2349. validation.rs:434's `#[allow(clippy::field_reassign_with_default)]` is anchored on `infer_profession_from_spec_names` (validation.rs:436), which builds no struct at all; its intended target `validate_gemini_build` (validation.rs:499) now uses struct-update syntax (`ValidatedBuild { explanation: .., ..ValidatedBuild::default() }`) and would not trip the lint either. Both are drift from the same doc-block displacement described in the orphaned-doc finding.

Remediation decision: Delete `#[allow(clippy::too_many_arguments)]` at engine.rs:2285 (llm_advisor already has its own at 2349) and delete `#[allow(clippy::field_reassign_with_default)]` at validation.rs:434. Confirm with `cargo clippy --workspace --all-targets -- -D warnings` that nothing new fires.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W123

- Task: T130 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:2410`.
- Dependencies: story entry gate.

Claim: Both LLM calls on the optimizer's shipping paths discard their error value entirely and leave no trace anywhere the user or a later log can see. engine.rs:2408-2414 (`llm_advisor`) matches `Err(_) => { return current; }`; engine.rs:2118-2125 (`optimize_deterministic_cancellable`) matches `Err(_e) => { }` with only the comment 'LLM explanation failed, keep the template explanation'. Neither logs (the optimizer crate has zero `log::` uses — verified by `grep -rn 'log::' crates/optimizer/src`), neither pushes a `ValidatedBuild::warnings` entry, and neither adds a `data::DataQualityReason`, even though both channels already exist and are used a few lines away (engine.rs:1860-1897 populates quality reasons for far smaller problems, such as unmodeled WvW effect sources). The `LlmError` value that would say *why* — expired key, 429 rate limit, Anthropic 529, timeout — is dropped on the floor at both sites.

Remediation decision: Capture the error and surface it through the existing result channels: in `llm_advisor` return a `(ValidatedBuild, Option<LlmError>)` or push onto `current.warnings` before returning; in `optimize_deterministic_cancellable` push a `data::DataQualityReason { field: "llm.explanation", .. }` (or a `result.validated.warnings` entry) carrying the provider message, so the About/Improve surfaces can show 'explanation unavailable: <reason>'.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W124

- Task: T131 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/engine.rs:2469`.
- Dependencies: story entry gate.

Claim: The advisor SWAP parser keeps a dead alternate grammar branch. Nothing in the repo emits `gear_prefix=`: `grep -rn --include=*.rs 'gear_prefix=' crates server` returns only this branch and its own comment. The prompt this parser answers is constructed 97 lines earlier in the same function (engine.rs:2379-2386) and offers exactly three forms — `SWAP: gear [slot] [prefix]`, `SWAP: gear [prefix]`, `SWAP: rune=[name]` — so the model is never told `gear_prefix=` exists. The branch body (engine.rs:2473-2486) is a near-verbatim copy of the bare-`gear ` branch at engine.rs:2453-2467: same `db.itemstat_by_name` lookup, same `fill_unlocked_gear_slots` with the same `PrefixRef` construction, same no-op skip.

Remediation decision: Normalize the old advisor token syntax at one parsing boundary and keep one branch, retaining compatibility coverage.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W125

- Task: T132 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gamedb.rs:742`.
- Dependencies: story entry gate.

Claim: Three lines of doc describing the `tests_alias_helpers` module have been glued onto the front of `profession_skill_index`'s doc comment. The function they describe is `pub(crate) mod tests_alias_helpers` at gamedb.rs:756-760 (consumed by crates/optimizer/src/data/boon_condition_formulas.rs:1205), which now carries no doc at all. `profession_skill_index` (gamedb.rs:753) has nothing to do with `is_condition` or alias routing — it buckets skills by profession and is documented correctly from line 745 onward.

Remediation decision: Move lines 742-744 down to sit above `#[cfg(test)] pub(crate) mod tests_alias_helpers` at gamedb.rs:755, leaving `profession_skill_index`'s doc starting at the (correct) 'Skills per profession...' line.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W126

- Task: T133 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini.rs:825`.
- Dependencies: story entry gate.

Claim: GeminiClient::generate_with_tools has zero callers anywhere. `grep -rn --include=*.rs '\.generate_with_tools(' crates server` returns nothing, and the unqualified grep returns only this definition, its own doc mention at line 841, an unrelated free function llm/mod.rs:196, and two println strings in tests/live_llm.rs. SymForge find_references(name='generate_with_tools', path='crates/optimizer/src/gemini.rs') reports no references. It is a pure forwarder to generate_with_tools_progress with a no-op progress closure — the same shape as the free helper in llm/mod.rs:196-203.

Remediation decision: Delete it; callers that want the no-progress form already go through llm::generate_with_tools.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W127

- Task: T134 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini.rs:970`.
- Dependencies: story entry gate.

Claim: The RateTracker mutex guard is taken on line 970 and persist_usage (line 971) is then called while it is still alive; persist_usage (1009-1021) does a blocking std::fs::write plus std::fs::rename — and on failure a std::fs::remove_file — inside the game process. The guard is only dropped when the block returns on line 972. Every other lock of self.rate in the file (935, 1026, 1035) is short and non-blocking, so this is the one site that holds it across disk I/O.

Remediation decision: Build the snapshot under the lock and drop it before writing: `let persisted = { let rate = self.rate.lock()...; rate.to_persisted() }; self.persist_usage(&persisted);` — i.e. change persist_usage to take PersistedUsage rather than &RateTracker.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W128

- Task: T135 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini.rs:1041`.
- Dependencies: W029.

Claim: The LLM response-cache and quota-reporting surface is dead in production. clear_cache is called only from llm/gemini.rs:231, which is the body of LlmClient::clear_cache (declared llm/mod.rs:192) — and `grep -rn --include=*.rs 'clear_cache' crates/addon` returns only `t("btn.clear_cache")` in settings.rs:1419-1424, whose handler clears gw2_api::cache::DataCache (settings.rs:1420), a different cache. Same shape for remaining_quota (gemini.rs:1024, also_at): reached only via LlmClient::remaining_quota (llm/mod.rs:189), and `grep -rn 'quota' crates/addon` finds no consumer — nothing in the UI displays it. The only live readers are gemini.rs's own tests and tests/live_llm.rs:197.

Remediation decision: Consolidate with W029; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W129

- Task: T136 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini_tools.rs:3`.
- Dependencies: story entry gate.

Claim: The module contract claims every tool reply stays under 500 tokens, and nothing in the file enforces or approaches it. exec_get_spec_traits (593-648) returns 3 minor plus 9 major traits, each with its full API description and up to 5 key_effects strings; exec_search_traits_by_effect (1072-1148) and exec_search_skills_by_effect (1246-1317) each cap at 20 results with descriptions; exec_get_optimizer_results (1013-1068) returns 5 candidates with ~25 numeric fields each plus spec and trait name lists; exec_get_trait_details (650-703) emits every fact and traited_fact through format_fact. `grep -n '500 tokens\|<500' crates/optimizer/src/gemini_tools.rs` finds this line and no truncation logic anywhere.

Remediation decision: Replace the claim with the real policy — results are capped by count (12/15/20/5) and the conversation is trimmed by gemini::trim_contents — or add an actual token/byte cap to the exec handlers if the 500-token contract is still wanted.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W130

- Task: T137 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini_tools.rs:27`.
- Dependencies: story entry gate.

Claim: The NamedRecord impl for ItemStat is unreachable. find_named is the trait's only consumer, and `grep -rn --include=*.rs 'find_named' crates server` shows exactly two production call sites, both non-ItemStat: gemini_tools.rs:596 (specializations.values()) and :653 (traits.values()); the four test call sites (2513, 2526, 2539, 2546) use Specialization and GW2Trait. The ItemStat path was cut when find_itemstat_by_name (line 93) was rewritten to delegate to db.itemstat_by_name. rustc cannot warn because find_named is generic and NamedRecord has live impls.

Remediation decision: Delete `impl NamedRecord for ItemStat` (lines 27-34) and drop the ItemStat import if it becomes unused; the trait doc at line 20 should stop naming ItemStat.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W131

- Task: T138 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/gemini_tools.rs:943`.
- Dependencies: story entry gate.

Claim: exec_score_build (943-993) is a copy of exec_simulate_combat (871-941): the same seven-step prologue appears verbatim in both — args["gear_prefix"] -> find_itemstat_by_name -> 'Stat prefix not found' error -> calculate_full_set_stats -> identical 'has no budget the game mode can price' error string -> base_stats() += gear_stats -> compute_derived (compare lines 872-891 with 944-963; the error format! strings at 882-887 and 954-959 are character-identical). It then hand-rolls a second copy of format_combat_performance inline as "combat_summary" (lines 983-991), and exec_get_optimizer_results does it a third time as "combat" (lines 1052-1062), each with a slightly different key set.

Remediation decision: Extract the prefix -> (itemstat, full_stats, derived) prologue into one helper returning Result<_, Value> so both execs share the error strings, and route all three combat renderings through format_combat_performance (with a field subset argument if the shorter forms are deliberate).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W132

- Task: T139 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:110`.
- Dependencies: story entry gate.

Claim: `grep -n stop_reason crates/optimizer/src/llm/anthropic.rs` shows the field is assembled by `read_anthropic_stream` (lines 171-229, 277) and read only by tests (1207, 1229); no production path consumes it, hence the `#[allow(dead_code)]`. The empty-response errors 'No content from Anthropic' / 'No text in Anthropic response' (anthropic.rs:636-637, 689) therefore drop the one field (`max_tokens`, `refusal`) that explains why a response was empty, while the OpenAI-compatible path reports it ('Empty response from {label} (finish_reason: {finish})', openai_compat.rs:351-354). openai.rs:379 has the same pattern on `ModelEntry.created`, which is deserialized and never read.

Remediation decision: Include `stop_reason` in the empty-content error text like the OpenAI path does and drop the allow; delete the `created` field from openai.rs's `ModelEntry` (serde ignores unknown fields by default).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W133

- Task: T140 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:110`.
- Dependencies: W132.

Claim: Two #[allow(dead_code)] sites in the batch. (a) MessagesResponse.stop_reason is assembled by read_anthropic_stream (anthropic.rs:226-231, 277) but never read on any production path (grep 'stop_reason' in anthropic.rs -> definitions, the reader, and tests at 1207/1229 only); a `max_tokens` truncation therefore looks identical to `end_turn`, whereas sse.rs surfaces finish_reason in its Empty diagnostic. (b) openai.rs:379-380 declares `created: Option<u64>` solely to be ignored; serde already skips unknown fields, so the field and its allow are pure weight.

Remediation decision: Consolidate with W132; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W134

- Task: T141 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:340`.
- Dependencies: W027.

Claim: AnthropicClient::send_messages (anthropic.rs:375-446) is a hand copy of openai_compat::send_chat's retry skeleton (openai_compat.rs:308-394) and has already drifted: MAX_RETRIES is 3 here vs max_retries: 2 in both OpenAI-compatible providers, and the first backoff is the literal `std::time::Duration::from_secs(5)` (anthropic.rs:373) rather than openai_compat's private INITIAL_RETRY_DELAY (openai_compat.rs:43). read_anthropic_stream (anthropic.rs:164-279) likewise re-implements sse::read_stream's line loop but drops the skipped-payload counter (anthropic.rs:186-189 is a bare `Err(_) => continue`) and the JSON-envelope sniff, so an Anthropic stream that yields nothing reports 'No content from Anthropic' with no diagnostics.

Remediation decision: Consolidate with W027; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W135

- Task: T142 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:559`.
- Dependencies: story entry gate.

Claim: In `validate_key` the `400 | 403 =>` arm (anthropic.rs:550) reports `status: 400` even when the response was a 403, so the user sees 'HTTP 400' for a permission failure. The comment above it (anthropic.rs:547-549) says '403 means key is valid ... Accept these as valid keys', but the code only accepts a 403 whose body contains one of four hard-coded words; meanwhile `validate_key_detailed` (anthropic.rs:609-613) accepts every 403 unconditionally. Both entry points are live: crates/addon/src/ui/setup.rs:444 calls `validate_key`, crates/addon/src/ui/main_view/tabs/settings.rs:176 calls `validate_key_detailed`, so the setup wizard and the Settings tab can disagree on the same key.

Remediation decision: Use the real `status` in the error, make the 403 policy identical in both methods (or let `validate_key` delegate to `validate_key_detailed` and map `valid` back to Ok/Err), and replace the inline word list with `has_billing_keyword`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W136

- Task: T143 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/body.rs:17`.
- Dependencies: story entry gate.

Claim: body.rs:17-18 and anthropic.rs:34 both state MAX_COMPLETION_TOKENS is 16_384; openai_compat.rs:55 defines it as 65_536 and its own doc (line 50) says 'Raised from 16_384'. The margin argument in body.rs ('two orders of magnitude under' the 8 MiB cap) no longer holds: 65_536 tokens x 20 bytes is about 1.3 MiB, roughly 6x under the cap, not 100x.

Remediation decision: Reference the constant by name without repeating its value, or restate the arithmetic against the current 65_536 (and note that MAX_LLM_BODY must be revisited if it grows again); fix anthropic.rs:34 the same way.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W137

- Task: T144 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/gemini.rs:19`.
- Dependencies: story entry gate.

Claim: The deferred work this ponytail hands off has landed: crates/optimizer/src/gemini.rs:417 wraps the reader in `body_capped(reader)` and gemini.rs:473 returns `body_cap_exceeded()` (same wording, gemini.rs:495-503) once the cap is hit. Every string the adapter receives from `generate`, `generate_cached` or `generate_with_tools_progress` is therefore strictly under MAX_LLM_BODY, so `text.len() as u64 > MAX_LLM_BODY` at llm/gemini.rs:23 can never be true. crate::gemini's own doc (gemini.rs:402-404) even notes the adapter check 'runs after this function has finished allocating'. The shim, its three `.and_then(body_capped)` call sites and the `body_cap_rejects_an_oversized_response` test now guard an unreachable branch behind a comment that says the real fix is still pending.

Remediation decision: Delete `body_capped` in llm/gemini.rs, the three `.and_then(body_capped)` calls (lines 184, 191, 212), the `MAX_LLM_BODY` import and the test at 336-342; rely on crate::gemini's reader cap.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W138

- Task: T145 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/gemini.rs:19`.
- Dependencies: W137.

Claim: The deferred work this ponytail hands off has landed: crates/optimizer/src/gemini.rs:417 wraps the socket in body_capped(reader) and :476 returns body_cap_exceeded() at the same MAX_LLM_BODY ceiling. The adapter-side body_capped(text: String) at gemini.rs:22-33 therefore can never observe text.len() > MAX_LLM_BODY (the inner reader errors first), and the surrounding doc at gemini.rs:13-15 ('the peak allocation there is not bounded by this check') is now false.

Remediation decision: Consolidate with W137; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W139

- Task: T146 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/gemini.rs:71`.
- Dependencies: story entry gate.

Claim: `grep -rn --include=*.rs '\.raw()' crates server` returns nothing: `GeminiLlmClient::raw` has no caller anywhere, including tests and examples. Its doc names a 'migration period' that ended; `pub` on a lib crate keeps rustc from flagging it.

Remediation decision: Delete `raw()` and its doc comment.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W140

- Task: T147 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/gemini.rs:71`.
- Dependencies: W139.

Claim: GeminiLlmClient::raw() is a self-described migration shim with zero callers: grep -rn '\.raw()' crates server returns nothing (checked addon, core, gw2api, optimizer incl. tests and examples, and server/).

Remediation decision: Consolidate with W139; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W141

- Task: T148 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/mod.rs:153`.
- Dependencies: story entry gate.

Claim: generate_brief's default discards max_tokens and calls generate(). Only OpenAiClient and OpenRouterClient override it; AnthropicClient does not, although its send_messages already takes a `max_tokens: u32` parameter (anthropic.rs:338) so the override is trivial. The trait doc says 'Providers without a per-call cap fall back to generate' -- Anthropic has a mandatory per-call cap. grep 'generate_brief(' -> engine.rs:2118 and :2408 call it with BRIEF_REPLY_TOKENS = 2_048 for the advisor lines and build explanation.

Remediation decision: Override generate_brief in AnthropicClient with send_messages(&messages, None, None, max_tokens.min(ANTHROPIC_MAX_TOKENS)); correct the trait doc to name Gemini as the only provider that falls back.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W142

- Task: T149 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/mod.rs:189`.
- Dependencies: W029.

Claim: LlmClient::remaining_quota has no production caller and its doc (mod.rs:181-188) describes a Settings UI consumer that does not exist. grep -rn 'remaining_quota(' crates server -> the trait decl, four impls, the Gemini adapter, and tests only. The Settings tab instead reads `<provider>_usage.json` from disk on its own and parses `requests_today` (crates/addon/src/ui/main_view/tabs/settings.rs:595-608), duplicating RateTracker's persistence format in the addon crate without the day-rollover check.

Remediation decision: Consolidate with W029; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W143

- Task: T150 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/mod.rs:195`.
- Dependencies: story entry gate.

Claim: The free function llm::generate_with_tools has no call site: grep -rn 'generate_with_tools(' crates server -> only its definition at mod.rs:196 and the unrelated inherent method crate::gemini::GeminiClient::generate_with_tools at gemini.rs:825. Both addon callers (chat_flow.rs, optimize_flow.rs) use generate_with_tools_progress directly.

Remediation decision: Delete the function.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W144

- Task: T151 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/openai.rs:54`.
- Dependencies: story entry gate.

Claim: The usage-file load in with_persistence (`read_to_string(..).ok().and_then(|s| serde_json::from_str::<PersistedUsage>(&s).ok())`, fall back to RateTracker::new) is copied verbatim into openai.rs:54-64, openrouter.rs:68-78, anthropic.rs:311-321 and a fourth time in crates/optimizer/src/gemini.rs:675-679. rate.rs:164-166 records that the *persist* half was deduplicated after GLM F20 but the load half was left in place. Every copy swallows a corrupt or unreadable file with .ok(), silently resetting the RPM window and daily counter, after which the next successful request overwrites the corrupt file so the failure is never observable.

Remediation decision: Add rate::load_usage(path: &Path, rpm_limit: u32) -> RateTracker next to persist_usage, have all four constructors call it, and surface a parse failure through the Nexus log at Warning rather than .ok().

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W145

- Task: T152 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/openai.rs:150`.
- Dependencies: story entry gate.

Claim: `validate_key` in openai.rs:150-153, openrouter.rs:167-170 and anthropic.rs:552-555 each carry an inline, case-sensitive keyword list, while `has_billing_keyword` in mod.rs:211 is the shared case-insensitive helper that the same files' `validate_key_detailed` already use (openai.rs:200/212, openrouter.rs:218/230, anthropic.rs:604; crate::gemini uses it too at gemini.rs:546). The inline lists lack 'payment', 'credit', and the Google status codes, and miss 'Billing' with a capital B. Same file also builds the identical `GET /models` request three times (openai.rs:134-141, 167-174, 350-357; openrouter.rs:151-158, 184-191, 374-383) where anthropic.rs factored it into `models_request()`; openrouter's `list_models` sends the Referer/X-Title headers that its two `validate_key*` copies do not.

Remediation decision: Replace each inline chain with `super::has_billing_keyword(&body)`; add a `models_request()` helper to openai.rs and openrouter.rs like anthropic.rs has.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W146

- Task: T153 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/openai.rs:150`.
- Dependencies: W145.

Claim: validate_key in openai.rs:150-154 and openrouter.rs:167-171 reimplements the billing-keyword check as a case-sensitive four-word list, anthropic.rs:552-556 uses a different four-word list, while validate_key_detailed in all three uses the shared case-insensitive has_billing_keyword() (mod.rs:211). Both entry points are live: grep shows setup.rs:430-451 calls validate_key() and settings.rs:176 calls validate_key_detailed(). anthropic.rs:560 additionally reports `status: 400` for a 403 response that lacked the keywords.

Remediation decision: Consolidate with W145; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W147

- Task: T154 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/openai.rs:418`.
- Dependencies: story entry gate.

Claim: The comment promises a newest-first ordering ('o3 before o1'); the code is a plain ascending string sort, which puts 'o1' before 'o3' and 'gpt-4o' before 'gpt-4o-mini' only by accident of prefix. openrouter.rs:422-424 and anthropic.rs:771-772 use the same sort and describe it honestly as alphabetical. `openai_display_name` (openai.rs:437-450) is a hand-maintained nine-entry table that drifts with every model release and only affects display.

Remediation decision: Change the comment to 'Sort alphabetically by id' like the other two providers, or implement the ranking the comment describes. Consider dropping `openai_display_name` and showing the raw id, as OpenRouter/Anthropic already fall back to.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W148

- Task: T155 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/llm/openai.rs:418`.
- Dependencies: W147.

Claim: The comment promises a 'newer/better first' order with 'o3 before o1'; the code is a plain alphabetical sort on id, under which 'o1' sorts before 'o3' and 'gpt-4.1' before 'gpt-4o'. The comment contradicts the adjacent line.

Remediation decision: Consolidate with W147; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W149

- Task: T156 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/referee.rs:49`.
- Dependencies: story entry gate.

Claim: `EHP_FLOOR_WVW` (L50) is documented as "kept for test backward-compatibility", but no test uses it. Repo-wide ripgrep for `EHP_FLOOR_WVW\b` across every file type returns exactly one hit: this definition. The referee's own test module imports EHP_FLOOR_PVE, EHP_FLOOR_PVP, EHP_FLOOR_WVW_HAVOC, EHP_FLOOR_WVW_ROAM and EHP_FLOOR_WVW_ZERG (referee.rs:1076-1078) and pointedly not this one.

Remediation decision: Delete `EHP_FLOOR_WVW` and its doc line.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W150

- Task: T157 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/builder.rs:20`.
- Dependencies: story entry gate.

Claim: Six `pub` items in the rotation module have no production caller. `grep -rn --include=*.rs 'build_rotation_skills(' crates server` matches only this definition — zero callers anywhere, tests included (all real call sites use `build_rotation_skills_for_context`). The other five are reachable only from their own #[cfg(test)] modules: `combat_model::simulation_window_ms` (62) — grep finds only its definition and 8 test assertions, production uses `simulation_window_ms_for_mode`; `combat_model::setup_window_ms` (75) — definition plus 2 test lines, production uses `setup_window_ms_for_mode`; `combat_model::resistance_negates` (192) — definition plus 5 test lines; `mod::strip_total` (196) — definition plus builder.rs:730/968 in tests; `simulator::simulate_against` (268) — definition plus 6 test call sites in simulator.rs.

Remediation decision: Delete `build_rotation_skills`. Mark the four test-only wrappers `#[cfg(test)]` or inline their bodies into the tests. For `resistance_negates`, either make `receive_condition` call it (replacing `condition_is_damaging`) or delete it — do not leave both.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W151

- Task: T158 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/builder.rs:656`.
- Dependencies: story entry gate.

Claim: Any skill whose lowercased description merely contains the substring 'barrier' is given an invented 1,000-point barrier. The adjacent comment (builder.rs:654-655) states the problem outright: 'The public skill endpoint commonly omits barrier coefficients. Heuristic Barrier/Healing are simulated at full value with no report flag.' That barrier is then consumed as real data by both simulators — simulator.rs:713-715 adds it straight into `total_healing`, and wvw_timeline.rs:1138-1139 pushes it through `apply_barrier` where it absorbs incoming damage and feeds `sustain_margin` and `repeatable` in the report.

Remediation decision: Label fallback barrier coefficients as heuristic and include them in coverage; do not invent authoritative sourced coefficients.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W152

- Task: T159 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/mod.rs:196`.
- Dependencies: story entry gate.

Claim: The boon-strip pulse count (count_per_pulse x window/interval, guarding interval == 0) is implemented three times with three different zero-interval fallbacks. `strip_total` (mod.rs:196-212) branches on `*interval_ms == 0` and returns `count_per_pulse`. builder.rs:435-438 writes `window_ms.max(interval_ms).checked_div(interval_ms).unwrap_or(1)`. wvw_timeline.rs:1157-1165 writes the same expression with `.unwrap_or(0)`, inside an `if *interval_ms == 0` branch that already makes the checked_div infallible, so that fallback is unreachable. Only `strip_total` — the shared helper that exists for precisely this — is never called outside builder.rs's tests (`grep -rn strip_total crates` returns 3 hits: the definition and two test lines).

Remediation decision: Call `strip_total(effect)` from wvw_timeline.rs:1157 and from builder.rs:435 (the latter needs a small helper taking the three numbers, since it has no SkillEffect yet), then delete the two inline copies and the unreachable `.unwrap_or(0)`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W153

- Task: T160 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/simulator.rs:643`.
- Dependencies: story entry gate.

Claim: The strike-damage fold (weapon_strength * effective_power / reference_armor() * dmg_multiplier * hit_count * crit_factor * strike_mult, with Might folded into power at 30/stack and Fury into the crit bonus) is written out four times: simulator.rs:637-650 (`use_skill`), simulator.rs:147-151 (`StaticCast::new`, the crit-free coefficient half), simulator.rs:1093-1107 (`skill_cast_value`), and wvw_timeline.rs:1071-1088 (`apply_skill_effect`). The code already knows: simulator.rs:1092 carries the comment 'Same power / crit / strike_mult fold as `use_skill`.'

Remediation decision: Extract one `fn strike_damage(params: &SimParams, might_stacks: f64, fury: bool, hit_count: u32, dmg_multiplier: f64) -> f64` in simulator.rs, make it `pub(super)` so wvw_timeline can use it too, and have all four sites call it. Replace the `30.0` literals with `boons().might_power_per_stack()` in the same pass.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W154

- Task: T161 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:59`.
- Dependencies: story entry gate.

Claim: Eight `WvwCombatReport` fields are computed by the timeline and never read. For each of successful_action_count (59), peak_protected_damage_5s (63), incoming_damage (66), avoided_damage (67), barrier_absorbed (69), conditions_cleansed (70), combo_activations (71) and secured_sequence_damage (80), `grep -rn --include=*.rs '\b<field>\b' crates server | grep -v rotation/wvw_timeline.rs` returns exactly one hit, and in every case it is a struct-literal initialiser inside referee.rs's `mod tests` (which opens at referee.rs:1069; the literal is at referee.rs:1120-1144). grouped_sheet.rs:364 builds its report with `..WvwCombatReport::default()` and names none of them. The struct derives only Debug/Clone/Default/PartialEq — no Serialize — so there is no serialisation consumer either. Contrast peak_protected_damage_2s (9 external refs) and secured_sequence_control_ms (4), which are genuinely read.

Remediation decision: Expose useful simulation counters via quality/diagnostic report consumers and remove only counters confirmed redundant.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W155

- Task: T162 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:428`.
- Dependencies: story entry gate.

Claim: `Timeline::new` hardcodes a 10-second weapon-swap cooldown that the only production caller immediately overwrites: `evaluate_wvw_timeline` builds the Timeline at wvw_timeline.rs:387-396 and then assigns `timeline.weapon_swap_cooldown_ms = weapon_swap_cooldown_ms;` at line 397 from `WvwTimelineInput`. The constructor already takes 8 arguments and carries `#[allow(clippy::too_many_arguments)]` at line 403 to silence the resulting lint.

Remediation decision: Pass `weapon_swap_cooldown_ms` through `Timeline::new` — or better, hand `Timeline::new` the whole `WvwTimelineInput` (it already owns every other argument), which removes both the post-hoc assignment and the `too_many_arguments` allow.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W156

- Task: T163 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:963`.
- Dependencies: story entry gate.

Claim: The scripted enemy's Condition Damage stat is the bare literal 1_800.0, repeated at wvw_timeline.rs:963 (on-skill-use Confusion), :1042 (incoming condition pulse) and :1050 (incoming leftover fraction). It has no named constant, no source citation, and does not scale with `WvwProfile`'s tier/kind pressure the way the enemy's strike damage does (for_scenario:161-163 derives `enemy_power` from `tier_pressure * kind_pressure`).

Remediation decision: Hoist it to one `const ENEMY_CONDITION_DAMAGE: f64 = 1_800.0;` next to the other consts at the top of the file with a note on where the number came from, or better, derive it from `WvwProfile`'s `pressure` the same way `enemy_power` is, and store it on the profile.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W158

- Task: T164 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scenario.rs:13`.
- Dependencies: story entry gate.

Claim: `ScenarioSpec.target_profile` is written by every constructor and read by nothing. grep -rn --include=*.rs 'target_profile' crates server returns 30 hits; every one outside scenario.rs is the literal `target_profile: TargetProfile::Single` in a struct initializer (addon optimize_flow.rs:199/1216, search_v2.rs:2056/2753/2928/2982, referee.rs:1342/1356/2132/2152/2180/2238/2266, rotation/wvw_timeline.rs:2289/3189, grouped_sheet.rs:329, examples). The only non-write is the assertion at scenario.rs:368. Consequently `TargetProfile::Cleave` and `TargetProfile::AoE` (scenario.rs:70-71) are never constructed anywhere in the repo.

Remediation decision: Remove unused target-profile state if fresh callers confirm no behavior; do not change existing cleave semantics.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W159

- Task: T165 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scenario.rs:113`.
- Dependencies: story entry gate.

Claim: Six `RoleObjective` variants — `WvWZergDps`, `WvWZergSupport`, `WvWDisruptor`, `PvPBurst`, `PvPSustain`, `PvPDisruptor` (scenario.rs:127-133) — are kept, by the comment's own admission, only so old mappings and tests compile. They are never constructed in production: the UI chip set is `PLAY_ROLES` (scenario.rs:138-146), which lists seven other variants, and the addon's only role source is `MainState.selected_role` (crates/addon/src/state.rs:497). The sole non-test reference is the pip-colour match at crates/addon/src/ui/main_view/mod.rs:624-627, which is itself unreachable for these arms. They still cost six arms in each of `label` (:161-166), `play_label` (via the `other =>` fallthrough), `profile_id_for` (:266-271), `combat_tier` (:279-284) and `combat_kind` (:295-308).

Remediation decision: Remove unreachable role variants after verifying serialized compatibility and profile reachability; keep a read migration if serialized.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W160

- Task: T166 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scenario.rs:220`.
- Dependencies: story entry gate.

Claim: Five public methods on this module have no production caller. `profile_id` (:222) — grep -rn 'profile_id' crates server shows every production hit is `profile_id_for` (addon optimize_flow.rs:205, examples necro_holes_check.rs:119, scourge_support_check.rs:104); `to_weights` (:332) — every production hit is `to_weights_for` (addon mod.rs:605, :642, :885); `RoleObjective::combat_tier` (:276) — grep for 'combat_tier()' returns nothing outside scenario.rs, and its doc claims it is 'used to construct ScenarioSpec' when the addon reads `state.main.wvw_combat_tier` instead; `ScenarioSpec::with_combat_kind` (:100) — no hits outside scenario.rs (`with_combat_tier` IS used, at synergy_pipeline.rs:2126); `CombatKind::label` (:43) — no hits for `combat_kind.label()` or `CombatKind::*label` anywhere.

Remediation decision: Delete `profile_id`, `to_weights`, `RoleObjective::combat_tier`, `with_combat_kind` and `CombatKind::label`; rewrite the handful of tests that call them (scenario.rs:377-421, :425-462, :537-552) to use `profile_id_for`/`to_weights_for` with an explicit tier, which is what production does and therefore what should be under test.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W161

- Task: T167 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scoring.rs:275`.
- Dependencies: story entry gate.

Claim: The comment says the presets are "now loaded from objective profile data", but `preset_power_dps` through `preset_celestial` (L279-367) return hardcoded struct literals and touch no profile data at all, and `PRESETS` (L370-377) is a hardcoded array of function pointers — which is what the shipping UI actually uses (crates/addon/src/ui/radar_chart.rs:289 iterates `OptimizationWeights::PRESETS`). data/objective_profiles.rs:4 repeats the claim: "Replaces the hardcoded `PRESETS`, `WEIGHT_BUDGET`, ...". Only `default_for_mode` (L242-258) genuinely consults profiles, and it falls back to `preset_balanced()`. The module header (L2-3) makes the same promise for the whole file: "a 6-axis radar-chart-driven scoring model driven by objective profile data files", while L646-651 and L657-660 document that the live rank path deliberately ignores the profile JSON norms.

Remediation decision: Rewrite both comments to state what is true today — presets are hardcoded and are what the UI uses; profile data currently supplies only `default_for_mode` and the (uncalled) ObjectiveScorer — or finish the migration so the presets really come from objective_profiles data. Fix the matching sentence in data/objective_profiles.rs:4.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W162

- Task: T168 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scraper.rs:27`.
- Dependencies: story entry gate.

Claim: `format_source_status` has no caller: grep -rn --include=*.rs 'format_source_status' crates server returns only the definition (scraper.rs:28) and its four tests (scraper.rs:1288-1319). The doc claims it is the Settings status line, but crates/addon/src/ui/main_view/tabs/settings.rs formats its own status and only calls `scrape_all_with_progress` (settings.rs:1598). Same file, same pattern: `pub fn scrape_all` (scraper.rs:59) is a thin wrapper whose only caller is the `#[ignore]`d network test at scraper.rs:1579 — production goes straight to `scrape_all_with_progress`.

Remediation decision: Either call `format_source_status` from settings.rs (it is better factored than the inline formatting there) or delete it with its tests. Delete `scrape_all` and change the ignored test at scraper.rs:1579 to call `scrape_all_with_progress(&tmp, &|| false, &|_, _| {})` directly.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W163

- Task: T169 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scraper.rs:84`.
- Dependencies: story entry gate.

Claim: `scrape_all_with_progress` contains three near-identical fan-out-to-three-sources early-return blocks: the cancel-at-entry block (scraper.rs:69-79), the client-build-failure block (:83-104) and the create_dir_all-failure block (:107-129). The two failure blocks each spell out three hand-written `ScrapeResult { source: "...".into(), builds: vec![], error: Some(msg.clone()) }` literals plus three hand-written `on_progress` calls with the same format string — roughly 46 lines that differ only in the message text. A `cancelled_result(source)` helper already exists at scraper.rs:233-240 and proves the shape is factorable; it is used by the cancel paths but not by the error paths.

Remediation decision: Add `fn all_failed(msg: &str, on_progress: &dyn Fn(&str, &str)) -> Vec<ScrapeResult>` alongside the existing `cancelled_result`, driven by a single `const SOURCES: [&str; 3] = ["snowcrows", "hardstuck", "guildjen"]`, and have all three early-return blocks (plus `cancelled_result`'s callers) go through it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W164

- Task: T170 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scraper.rs:276`.
- Dependencies: story entry gate.

Claim: The nine GW2 profession names are written out four separate times inside this one file: `SC_PROFESSIONS` here, `HS_PROFESSIONS` at scraper.rs:449-459 (identical lowercase list), an inline `let professions = [...]` inside `parse_profession_spec_from_url` at scraper.rs:817-827 (identical lowercase list), and `const CORE_PROFESSIONS` inside `extract_traits` at scraper.rs:1062-1072 (same nine, title-cased). A fifth copy of the same list lives at crates/optimizer/examples/flow_calibration.rs:19-29. The list is a fixed property of the game, and `GameDb.professions` already holds it at runtime.

Remediation decision: Declare one `const PROFESSIONS: &[&str]` at module scope, point `scrape_snowcrows`, `scrape_hardstuck` and `parse_profession_spec_from_url` at it, and derive `CORE_PROFESSIONS` from it via the existing `title_case` helper rather than re-typing the title-cased spellings.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W165

- Task: T171 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/scraper.rs:1118`.
- Dependencies: story entry gate.

Claim: `strip_tags` is a character-for-character duplicate of `text_util::strip_gw2_markup` (crates/optimizer/src/text_util.rs:197-209) — same `in_tag` state machine, same match arms, same accumulator — differing only in that the shared helper additionally normalizes U+00A0 to a space. `text_util` is in the same crate and is already imported by combat.rs:12, synergy.rs:12 and upgrade_graph.rs:20, so there is no visibility reason for the copy.

Remediation decision: Delete `strip_tags` and its test (scraper.rs:1416-1420), `use crate::text_util::strip_gw2_markup;` and call that at the one use site (scraper.rs:1103).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W166

- Task: T172 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:91`.
- Dependencies: story entry gate.

Claim: NudgeBudget's doc states the beam budget is 1,500 evaluations; SearchConfig::default() (line 63) sets eval_budget: 200_000 and its own comment explains 1500 was the OLD value. Likewise refine_piece_swaps' doc (line 172, 'The beam runs ~2 generations on default config') predates the patience/rotation-cycle stopping rule (lines 887-900), under which the beam runs until a full rotation cycle is flat (up to 24 generations).

Remediation decision: Reword both docs to reference SearchConfig::default() symbolically ('the beam's eval_budget/time_limit') rather than restating numbers that have since changed.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W167

- Task: T173 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:336`.
- Dependencies: story entry gate.

Claim: generate_neighbors has no production caller: `grep -rn 'generate_neighbors(' crates server` shows only the definition and nine #[cfg(test)] call sites in search_v2.rs. optimize_v2_search (line 804) calls generate_neighbor_groups directly. The doc block above it (line 314: 'six atomic mutation operators'; 322-333: describes the ~80 round-robin `take` admission) describes both a wrong operator count (13 groups are pushed) and an admission scheme that group_quotas/sample_group replaced.

Remediation decision: Make it #[cfg(test)] (or move the interleave into the test module) and rewrite the doc to point at generate_neighbor_groups + group_quotas as the live admission path.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W168

- Task: T174 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:772`.
- Dependencies: story entry gate.

Claim: Seven funnel-diagnostic counters (fn_generated, fn_admitted, fn_distinct HashSet, fn_beat, fn_tied, fn_worse, fn_first_diff) are maintained inside the beam's inner evaluation loop and emitted as OptimizeProgress.stage strings prefixed 'search_v2' (lines 758, 943, 951). The addon demultiplexes them by `progress.stage.starts_with("search_v2")` (crates/addon/src/ui/main_view/optimize_flow.rs:249) into nexus::log. The comment at 726-729 says the question they answered is closed ('Answered 2026-09-04') but keeps them.

Remediation decision: Add an explicit `diagnostic: bool` (or a separate `on_diagnostic` callback) to OptimizeProgress instead of prefix sniffing, and gate the fn_* histogram behind that or remove it now that the 2026-09-04 question is answered.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W169

- Task: T175 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:1451`.
- Dependencies: story entry gate.

Claim: swap_utility_skills' doc says a skill with slot None is eligible if it appears in the profession's palette list. The code at line 1494-1497 (`let slot_ok = skill.slot.as_deref() == Some("Utility"); if !slot_ok { return false; }`) rejects slot None unconditionally.

Remediation decision: Delete the '**or** the slot is None' clause from the doc (the strict rule matches eligible_slot_skills and refill_bar).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W170

- Task: T176 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:1469`.
- Dependencies: story entry gate.

Claim: The utility-skill eligibility filter (slot == "Utility", skill_palette_id != 0, specialization gated by equipped specs) plus the 'held' id set and the 3-slot neighbour loop are implemented twice nearly verbatim in swap_utility_skills (1469-1535) and swap_utilities_for_failed_gates (1607-1660); the same filter appears a third time generically in eligible_slot_skills (1334-1362) and a fourth time in refill_bar's `pick` closure (1854-1868).

Remediation decision: Route swap_utility_skills, swap_utilities_for_failed_gates and refill_bar through eligible_slot_skills(candidate, db, prof, "Utility") and one shared `propose_utility_swaps(build, choices, held)` helper.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W171

- Task: T177 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/search_v2.rs:1998`.
- Dependencies: story entry gate.

Claim: swap_weapons stops after 16 emitted neighbours. land_weapon_combos sorts two_hand/main/off alphabetically (1948-1950) and the loop emits up to two builds per combo, so the cap admits roughly the first eight alphabetical combos every generation; weapons late in the alphabet (Staff, Sword, Torch, Warhorn) are unreachable by this operator regardless of eval budget, which is exactly the 'same head slice forever' failure the file's own sample_indices doc (401-414) describes for the other operators. The number has no name and no comment.

Remediation decision: Name it (e.g. WEAPON_NEIGHBOUR_CAP) with the reason, or remove it and let group_quotas/sample_group stride the full combo list like every other operator group.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W172

- Task: T178 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/sigil_slots.rs:51`.
- Dependencies: story entry gate.

Claim: Eight of this module's public methods have no caller outside their own tests. grep -rn over crates/ + server/ shows the only production consumer is crates/optimizer/src/validation.rs (lines 15, 77, 229-238, 254-257), which uses exactly `SigilSlots::default()`, `set`, `get`, `is_empty`, `active_set` and `SigilSlot::ALL`. Unused: `SigilSlot::is_active_set` (:51), `SigilSlot::gear_slot` (:56), `SigilSlot::from_gear_slot` (:67), `SigilSlots::new` (:91), `SigilSlots::as_array` (:106), `SigilSlots::second_set` (:123), `SigilSlots::equipped` (:134), `SigilSlots::count_equipped` (:141). Each has a dedicated test (sigil_slots.rs:155-232), so the suite is green and rustc cannot warn on `pub` items in a lib.

Remediation decision: Delete the eight unused methods and the tests that exist only for them (`seats_round_trip_through_their_weapon_slot`, and the `as_array`/`count_equipped`/`equipped`/`second_set` assertions inside the remaining tests). Keep `SigilSlots::new` only if a test constructor is wanted, and gate it `#[cfg(test)]` if so.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W173

- Task: T179 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/stats.rs:147`.
- Dependencies: story entry gate.

Claim: Ten public items across this batch have no caller anywhere in crates/, server/ or examples/ (verified with SymForge find_references and `grep -rn --include=*.rs` including tests). `stats::armor_weight` (this line): zero references repo-wide — every `armor_weight(` hit is the unrelated `profession_profiles::armor_weight` method called as `p.armor_weight(..)` inside data/profession_profiles.rs and its tests. The others are listed in also_at: scoring.rs:201 `budget_remaining` and :206 `budget_remaining_with` (zero references), combat.rs:81 `total_condi_duration_for` (zero), referee.rs:126 `first_failure` (zero), combat.rs:287 `condition_duration_multiplied` and :335 `boon_duration_multiplied` (definitions plus their own combat.rs tests only — no production caller), stats.rs:45 `is_zero` (its own test only, stats.rs:870-874), stats.rs:73 `impl AddAssign for StatBlock` (the by-value impl — even its own test at stats.rs:865 uses `base += &bonus`, the &-impl; no by-value `+=` on a StatBlock found repo-wide).

Remediation decision: Remove confirmed unused stat APIs and exclusive tests, preserving production duration helpers and regression coverage.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W174

- Task: T180 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/stats.rs:348`.
- Dependencies: story entry gate.

Claim: `calculate_trait_stats` (L352) is a public wrapper kept for callers that no longer exist. `grep -rn --include=*.rs 'calculate_trait_stats(' crates server examples` returns only: the definition (stats.rs:352), three of its own tests (stats.rs:970, 1015, 1129), and TWO source-scanning meta-tests that assert production must never call it — engine.rs:4310 `!production.contains("stats::calculate_trait_stats(&")` and synergy_pipeline.rs:2847 `!chunk.contains("calculate_trait_stats(&")`. The doc comment's claim of "leftover callers (legacy spec precompute)" is false as of this tree.

Remediation decision: Delete `calculate_trait_stats`, port its three tests onto `calculate_trait_stats_for_mode(..., &GameMode::PvE)`, and delete the two source-scanning guard tests in engine.rs and synergy_pipeline.rs that only exist to police it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W175

- Task: T181 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/stats.rs:431`.
- Dependencies: story entry gate.

Claim: `mode_split_trait_attribute` (L434-476) embeds a 25-row balance table — numeric GW2 trait IDs mapped to per-mode attribute values (e.g. `(2371, "BoonDuration") => by_mode(180.0, 60.0, 60.0)`, `(413, "BoonDuration") => by_mode(240.0, 75.0, 75.0)`, plus twelve zeroed conditional/pet-only rows) — directly in Rust source, with provenance recorded only as a prose comment "verified against the live API rows on 2026-08-23".

Remediation decision: Move the table to data/trait_mode_splits.json with the project's standard loader (include_str! + OnceLock + validation) and per-row `sources`/`evidence_level`, matching profession_profiles.json. Keep `mode_split_trait_attribute` as the lookup shim over that data.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W176

- Task: T182 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/stats.rs:601`.
- Dependencies: story entry gate.

Claim: The same nine-field StatBlock summation is written out four times in this file: `impl AddAssign for StatBlock` (L73-85), `impl AddAssign<&StatBlock> for StatBlock` (L87-99), the free fn `add_block` (L634-644), and this inline copy in `calculate_full_stats` (L601-609) that sits 33 lines above four calls to `add_block` (L613, 617, 621, 625) doing exactly the same thing. A fifth copy exists outside this batch at synergy_pipeline.rs:1384-1392.

Remediation decision: Keep `impl AddAssign<&StatBlock>` as the single implementation; replace the inline block at L601-609 with `stats += &gear;`, replace `add_block` calls with `+=` and delete it, and delete the unused by-value `impl AddAssign` (see the dead-items finding). Fix synergy_pipeline.rs:1384-1392 the same way.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W177

- Task: T183 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy.rs:149`.
- Dependencies: story entry gate.

Claim: SynergyLink.source, target, source_name, target_name and link_type (and the SynergyLinkType enum) are write-only in production. `grep -rn 'synergy_links\|SynergyLink\b\|link_type\|SynergyLinkType' crates server` outside synergy.rs/synergy_pipeline.rs returns nothing; inside synergy_pipeline.rs links are only .extend()ed, sorted by .score (line 392) and passed to template_explanation, which reads only .description. Only tests read .source/.target. The doc at line 707 ('so UI can render which component completes a chain') describes a consumer that does not exist.

Remediation decision: Trim unused synergy-link metadata after checking graph/UI consumers; preserve score and explanation.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W178

- Task: T184 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy.rs:265`.
- Dependencies: story entry gate.

Claim: extract_sigil_effects hardcodes Sigil of Force (+5% PvE / +3% competitive) and Sigil of Bursting (+6% / +4%) as a name-match fallback. combat.rs:1382-1386 (parse_sigil_modifier) hardcodes the identical table with identical competitive split. Both exist as fallbacks for items lacking description/buff text, i.e. for test fixtures with `details: None` (synergy.rs test_extract_sigil_force, referee.rs:1185).

Remediation decision: Have extract_sigil_effects call combat::parse-side logic (apply the shared DamageModifiers path via effects_from_upgrade_text on the same text combat.rs uses) so the fallback table lives once, or move the two numbers into data/ with the other balance constants.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W179

- Task: T185 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:46`.
- Dependencies: story entry gate.

Claim: SynergyCandidate.elite_spec is written at line 425 (`elite_spec: *elite`) and in three test constructors, and never read. `grep -rn '\belite_spec\b' crates server` shows every other hit is a different type (engine::BuildCandidate.elite_spec, rotation_profiles). select_weapons re-derives the elite spec from spec_ids via db.specializations instead (lines 678-688).

Remediation decision: Delete the field and the #[allow], or use it in select_weapons in place of the db lookup.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W180

- Task: T186 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:86`.
- Dependencies: story entry gate.

Claim: The doc on optimize_synergy says it is kept for search_v2::optimize_v2_search and that 'that call site should move to optimize_synergy_cancellable'. It already did: search_v2.rs:657 and engine.rs:1975 both call optimize_synergy_cancellable. `grep -rn 'optimize_synergy(' crates server` shows the non-cancellable wrapper is now called only by examples/flow_calibration.rs:55, examples/necro_holes_check.rs:124 and one test (synergy_pipeline.rs:2102).

Remediation decision: Rewrite the doc: 'Non-cancellable convenience wrapper used by examples and tests; production callers use optimize_synergy_cancellable.'

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W181

- Task: T187 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:744`.
- Dependencies: story entry gate.

Claim: select_weapons' set-2 selection (744-786) is a copy of the set-1 selection (702-741): same two-hand branch, same main+offhand double loop, same main-only fallback, differing only in the 'skip set 1's main' guard at 748-750. The comment at 776-778 even documents that the main-only branch had to be back-ported to the copy after it was missed.

Remediation decision: Extract `best_land_set(available, weapon_scores, exclude_main: Option<&str>) -> ((Option<String>, Option<String>), f64)` and call it twice.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W183

- Task: T188 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/src/validation.rs:308`.
- Dependencies: story entry gate.

Claim: `ValidatedBuild::resolve_slot_prefix_ids` (validation.rs:332) is a public method with no production caller, guarded by a TODO that says so itself. `grep -rn --include=*.rs 'resolve_slot_prefix_ids' crates server` returns exactly three hits: the definition (validation.rs:332), one call inside the `#[cfg(test)]` module (validation.rs:2748, tests start at 1753), and a passing doc mention in crates/core/src/types.rs:597. The TODO at line 308-311 states the condition plainly — 'No caller builds a ValidatedBuild from SavedBuild yet; Task 3 must settle the backfill when weapon presence is known' — and its own doc (lines 320-323) concedes the better door already exists: 'gw2_core::types::GearSlots::from_legacy_with is the better door — it resolves at construction, so the zero-id state never exists'. The 30-line doc block also states the same 'Resolve migration-produced zero itemstat ids' paragraph twice, at 304-307 and again at 312-318.

Remediation decision: Route saved-build conversion through the existing canonical slot migration, then remove redundant uncalled backfill helper.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W184

- Task: T189 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `crates/optimizer/tests/objective_profiles_integration.rs:476`.
- Dependencies: W083.

Claim: This test pins a serde back-compat shim for a field name that no longer exists in the schema: `#[serde(alias = "disable")] pub control: f64` at crates/optimizer/src/scoring.rs:104, documented there as 'Control axis (replaces old "disable" axis). Backward-compatible: deserializes from "disable" in old saved data.' Nothing in the repo writes `"disable"` — grep -rn '"disable"' over crates/ finds only the alias attribute and this test. The 5-axis JSON in the test (no `boon_support`) is likewise a pre-6-axis save format that nothing produces.

Remediation decision: Retain disable alias for saved-build reads; canonicalize on save and test the migration.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W185

- Task: T190 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `docs/superpowers/plans/2026-08-24-feedback-server.md:1290`.
- Dependencies: story entry gate.

Claim: This 1820-line tracked plan embeds a full copy of the feedback server's source (config.rs, error.rs, app.rs, main.rs, ids.rs, reports.rs, ratelimit.rs, admin.rs, Dockerfile, compose.yml, .env.example, backup.sh) that has since diverged from the shipped code on security-relevant points, while every one of its checkboxes is still `- [ ]` even though the server is deployed. Divergences: line 714 specifies the client IP as the 'first value of X-Forwarded-For', which the shipped reports.rs:43-62 deliberately reversed to the rightmost entry behind an opt-in `trust_xff` flag (spoofing fix, tested at api.rs:726); lines 1290-1291/1321/1420 specify and test auto-read-on-fetch that the shipped server refuses to do; the embedded Config (line ~247) has no admin_user/admin_password/session_secret/trust_xff; the embedded error.rs has no DbUnavailable; the embedded compose.yml carries a `build:` block and a ghcr image that the shipped compose.yml and deploy/README.md:16 explicitly say do not exist.

Remediation decision: Mark the plan as a historical artifact (status header + 'code blocks are the 2026-08-24 snapshot, not current') or strip the inline source blocks and point at server/feedback/src/ instead; at minimum fix the XFF and auto-read lines, which contradict deliberate later fixes.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W186

- Task: T191 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `docs/superpowers/plans/2026-08-26-per-slot-gear-implementation.md:141`.
- Dependencies: story entry gate.

Claim: A tracked planning document contains an agent's raw internal monologue about its own output plumbing - deliberating over `result.data`, incremental vs terminal yields, and what 'the caller sees'. It runs from line 139 ('Severity table format per instructions: severity | spec section | ...') through line 141 and is followed by a `---` and then the real Task 1. Lines 118-137 above it are likewise an unedited adversarial-review dump ('Findings table severity assignment: BLOCKER 1 ...', 'Also positive observation requirement: name what the code does well - e.g., ...') pasted into the middle of the plan.

Remediation decision: Delete lines 139-141 and either delete or properly format lines 118-137 as the review-findings appendix they are meant to be.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W187

- Task: T192 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `docs/superpowers/specs/2026-08-24-feedback-and-about-design.md:360`.
- Dependencies: story entry gate.

Claim: The design spec for the shipped feedback server states behaviour the server deliberately does not have, and excludes a feature that shipped. (1) Auto-read-on-fetch: admin.rs `list`/`get_one` do not call `mark_read`; there is a separate explicit `POST /v1/admin/reports/read` route (admin.rs:385) that the HTML calls, and tests/api.rs asserts the opposite of the spec at lines 556 ('GET one must not mark read'), 743 ('GET list must not mark read') and 838 ('GET list must leave every row received'). Line 285 repeats the false claim ('Admin `GET` auto-flipping `received` -> `read` is a mutating read by design'). (2) Line 32 lists 'Admin web UI beyond a token-gated JSON API' as explicitly out of scope, but server/feedback/src/admin.html is a 313-line admin web UI with its own login/logout/me session endpoints (admin.rs:374-380). (3) Line 369 says the server uses 'compile-time checked queries'; every query is a runtime `sqlx::query`/`query_as` with `.bind`, a change the implementation plan documents as deliberate.

Remediation decision: Add a short 'as-built deltas' section to the spec (or update §2, §6, §6a and §7 in place) recording: read is explicit via POST /reports/read, a session-cookie admin page ships, and queries are runtime-bound by design.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W188

- Task: T193 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `docs/superpowers/specs/2026-08-26-per-slot-gear-design.md:77`.
- Dependencies: W083.

Claim: The one-release migration window this spec (dated 2026-08-26) opened has never been closed. crates/core/src/types.rs still declares all three on SavedBuild: `stat_prefix: String` (:621), `gear_prefixes: GearPrefixGroups` (:625) and `slot_prefixes: Option<GearSlots>` (:629), and `GearPrefixGroups` is still referenced by 5 files (core/types.rs, core/storage.rs, addon/tabs/saveload.rs, optimizer/validation.rs, optimizer/grouped_sheet.rs). The workspace version is now 1.11.26 - roughly thirty patch releases past the promised removal. Line 84 of the same spec also says GearPrefixGroups would be 'marked `#[deprecated]`, and never constructed by new code'; it carries no deprecation attribute (types.rs:335-336).

Remediation decision: Either execute Task 7 of the implementation plan (stop writing the legacy fields, keep only legacy deserialization) or update the spec to state that both shapes are permanent and say which is authoritative.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W189

- Task: T194 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `locales/en.json:419`.
- Dependencies: W084, W091, W059.

Claim: 66 of the 683 locale keys are unreachable: the key string occurs nowhere in crates/, data/, docs/superpowers/, .github/ or README.md, and no code builds them dynamically (`grep -rn 'format!("stat\.'` etc. returns 0 for every one of these prefixes; the only dynamic key construction in the addon is `format!("choice.{id}")` at wizard.rs:54/1101 and about.rs:334, `format!("radio.genre.{key}")` at tabs/radio.rs:119, and taxonomy-driven `t(&key)` at wizard.rs:802 - I excluded the whole choice./cat./step./radio.genre. families from the count for that reason). Dead families: stat.* (11), fmt.* (11), slot.* (10), section.* (6), table.* (4), note.* (4), tradeoff.* (4), label.* (3), info.* (3), news.hint + news.layout.by_source + news.layout.timeline, tier.party + tier.squad, save.hint + save.none, about.fmt.remove, btn.refresh_data, settings.news_reading. They read as the leftovers of an earlier Stats/tradeoff panel ("Tradeoff Analysis", "ROTATION (simulated)", "Effective HP: {n}").

Remediation decision: Add locale reachability/parity validation with an explicit dynamic-key inventory; remove only keys proven unreachable.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W190

- Task: T195 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `server/feedback/src/admin.html:56`.
- Dependencies: story entry gate.

Claim: The admin page hardcodes the six taxonomy category ids (bug, wrong_build, wish, question, praise, coffee - lines 57-64) as static <option> elements, duplicating data/feedback_taxonomy.json. The whole point of the taxonomy tables and `PUT /v1/admin/taxonomy` (admin.rs:342-368) is that categories are data and can be replaced at runtime without shipping code - tests/api.rs:657-704 exercises exactly that by adding a 'translation' category. After such a change the inbox filter cannot select the new category, and the page itself already fetches `/v1/taxonomy` nowhere.

Remediation decision: Populate the category <select> at load time from `GET /v1/taxonomy` (the endpoint is already public and unauthenticated), falling back to the current static list if the fetch fails.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W191

- Task: T196 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `server/feedback/src/admin.html:83`.
- Dependencies: story entry gate.

Claim: The reply-markup line format is parsed by two independent copies of the same regex in the same file: `renderBody()` at line 83 and `parseLine()` at line 136 (`const m = String(line).match(/^%([BN])([LCR])([0-4])([PUO])\|(.*)$/);`). The two then decode the captures differently - renderBody maps m[2] through {L:'left',C:'center',R:'right'} and m[4] through a bullet/number table, parseLine keeps the raw letters - so the format is effectively specified twice, plus a third time in `packLine()` at line 141 which re-emits it by string concatenation.

Remediation decision: Extract one `const LINE_RE` plus a single parse/pack pair and have `renderBody` render from the parsed object instead of re-matching.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W192

- Task: T197 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `server/feedback/src/admin.html:308`.
- Dependencies: story entry gate.

Claim: Two of the four load() call sites drop errors into the console instead of the UI. Line 308 (status/category filter change) and line 309 (Enter in the search box) use `.catch(console.error)`, while the Reload button at line 307 handles the same failures properly - it maps `unauthorized` to `lock('Session expired. Log in again.')` and otherwise paints the message into #list. After a session expires, changing the filter therefore leaves the previous rows on screen with no indication that the list is stale and no login prompt.

Remediation decision: Route lines 308-309 through the same handler used on line 307 (extract it into a named `reload()` and reuse it for the Reload button, the selects, and the search box).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W193

- Task: T198 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `server/feedback/src/admin.rs:154`.
- Dependencies: story entry gate.

Claim: The login rate limiter keys on the raw client IP, while every other limiter key in the service uses the daily-rotating hash: reports.rs:110-112 and reports.rs:219-221 both compute `ip_hash(&ip, salt, today)` first and key on `ip:{hash}`. `login` calls `client_ip(...)` at admin.rs:148 and drops the result straight into the key, so plaintext IP addresses of anyone who hits /admin/login sit in the process-global RateLimiter map (up to MAP_CAP=10_000 entries) for the 15-minute window. The design deliberately never stores a raw IP anywhere else - tests/api.rs:180 asserts `assert_ne!(ip_hash, "203.0.113.5", "raw ip must never be stored")`.

Remediation decision: Hash first: `let hash = ip_hash(&ip, &s.config.ip_salt, Utc::now().date_naive());` and key on `login:{hash}`, matching reports.rs.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W194

- Task: T199 [US4]. Status: **planned**.
- Audit: S3, confirmed; location: `server/feedback/src/reports.rs:65`.
- Dependencies: story entry gate.

Claim: `version_at_least` silently discards every component of `X-Addon-Version` that does not parse as u32, then compares the surviving vectors lexicographically, so the version gate both over- and under-accepts. `X-Addon-Version: v1.6.0` parses to [6, 0] (the 'v1' component is dropped) and [6,0] >= [1,6,0], so a malformed header sails through the MIN_ADDON_VERSION check; conversely a legitimate two-component `1.6` parses to [1, 6], which is lexicographically less than [1,6,0], so a client is told 426 UpgradeRequired ('update the addon') for a version that is not actually below the minimum. Nothing reports the dropped components.

Remediation decision: Parse into a fixed (major, minor, patch) tuple with an explicit error - reject a header that does not parse with `ApiError::BadRequest` rather than filtering components away, and zero-extend a short version instead of comparing ragged vectors.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Demonstrate the claimed defect/debt at current symbols, verify the selected correction through its consumer, and record checks or current-code refutation.

### W195

- Task: T200 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/feedback/client.rs:200`.
- Dependencies: story entry gate.

Claim: The 1 MiB response cap is written as a bare `1024 * 1024` at three separate call sites in this file (lines 200, 247, 261) while every neighbouring module names its cap once as a documented const: `news.rs:11 MAX_BYTES`, `news_art.rs:21 MAX_BYTES`, `radio/logos.rs:38 MAX_BYTES`, `radio/directory.rs:48 MAX_BODY_BYTES`. The same file also hardcodes `.take(50)` (client.rs:225) for the per-request id limit, which must agree with `MAX_POLL_IDS: usize = 50` in feedback/tasks.rs:459 — two independent literals for one server contract, each with its own test.

Remediation decision: Add `const MAX_BODY_BYTES: u64 = 1024 * 1024;` next to `TIMEOUT`/`MAX_REASON_CHARS` at the top of client.rs and use it at all three sites; move `MAX_POLL_IDS` into client.rs (or import it from tasks.rs) so `.take(MAX_POLL_IDS)` and `pollable_ids` read the same constant.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W196

- Task: T201 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/news.rs:34`.
- Dependencies: story entry gate.

Claim: Three parallel fixed-size arrays are indexed by `NewsSource::index()` (crates/core/src/config.rs:134-142) with the array length `5` spelled as a bare literal in each of the three declarations, and nothing links the two. `NewsState::items`, `set_feed`, `note_fetch_failure`, `invalidate` and `needs` all index unchecked (`self.feeds[src.index()]`, news.rs:53/57/66/71/82), so a sixth `NewsSource` variant whose `index()` returns 5 compiles cleanly and panics on the render thread the first time that source is touched — there is no exhaustiveness check to catch it.

Remediation decision: Add `pub const COUNT: usize` to `NewsSource` beside `index()` and declare the arrays as `[_; NewsSource::COUNT]`, so adding a variant either compiles correctly or fails at the definition rather than at runtime.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W197

- Task: T202 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/radio/logos.rs:322`.
- Dependencies: story entry gate.

Claim: Declared shortcut with a stated ceiling: `cached_file` returns a hit without touching the file's mtime, so `evict_oldest` (logos.rs:368, cap `MAX_CACHE_FILES = 500`) can delete a frequently-used favorite's logo purely because 500 newer files were written. The stated ceiling has not been hit — the consequence is a re-download next session and, in the meantime, the letter-plate fallback that `slot_path_or_enqueue` (logos.rs:236-239) already handles for a Ready-but-missing file, with `release_pending_allows_requeue_ready_returns_path` pinning that behaviour.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W198

- Task: T203 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/state.rs:326`.
- Dependencies: story entry gate.

Claim: The only `// ponytail:` marker in this batch. It records a deliberate shortcut in `join_bounded`: workers still running when `UNLOAD_JOIN_BUDGET` expires are detached rather than joined, with cancel-aware HTTP explicitly deferred.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W199

- Task: T204 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/chat_bar.rs:559`.
- Dependencies: story entry gate.

Claim: `load_history` re-implements `trim_history` (chat_bar.rs:59-64) inline, line for line, on a local `mut hist: Vec<ChatMessage>` that `trim_history(&mut hist)` would accept unchanged.

Remediation decision: Replace the inline block with `trim_history(&mut hist);`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W200

- Task: T205 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/comparison.rs:861`.
- Dependencies: story entry gate.

Claim: `render_defenses` takes a `&ComparisonState` it never touches - the underscore prefix is the author acknowledging that. The sole caller (comparison.rs:585) still threads `comparison` through to it. The doc directly above explains the values that *would* have needed it ("Effective HP and Damage Reduction are shown per-tier in Combat Performance") were moved elsewhere.

Remediation decision: Drop the parameter and the argument at line 585.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W201

- Task: T206 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/fonts.rs:168`.
- Dependencies: story entry gate.

Claim: This range gate is a hand-maintained second copy of `LATIN_RANGES` (fonts.rs:33-35), which the doc comment itself flags: "this MUST mirror `LATIN_RANGES` exactly". Nothing enforces the mirror - the only range test in the file (`latin_ranges_are_zero_terminated_pairs`, line 320) checks the array's shape, not that the predicate agrees with it.

Remediation decision: Derive the predicate from `LATIN_RANGES` (iterate the pairs until the 0 terminator) so there is one source of truth, or add a test asserting the two agree on the boundary code points of every pair.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W202

- Task: T207 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/fonts.rs:306`.
- Dependencies: story entry gate.

Claim: A literal `C:\Windows` absolute path in tracked source, used as the fallback when the `WINDIR` environment variable is unreadable.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W203

- Task: T208 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/optimize_flow.rs:66`.
- Dependencies: W077.

Claim: Ledger entry, not a re-argument: seven `// ponytail:` deliberate deferrals in this batch, each with a stated ceiling. (1) optimize_flow.rs:66 — `start_optimization_with_profession` keeps a name that predates the explicit entry flag. (2) optimize_flow.rs:557 — `WORKER_REFUSED_ERROR` is untranslated English pending a `locales/` key. (3) optimize_flow.rs:819 — `ImproveOutcome` is recovered by string-comparing the rendered UI label (`from_label`) instead of being a field on `BuildSuggestion`. (4) optimize_flow.rs:837 — `KEPT_GEAR_HEADLINE` untranslated, same `locales/` blocker. (5) optimization.rs:1303 — `scoring::GEAR_PROFILES` is private, so `prefix_request_is_affirmative` can only re-check the one name returned, and "don't use minstrel, give me celestial" falls back to the model's pick. (6) build_display.rs:100 — one process-wide `thread_local` stance preview index, valid only while Improve never shows two skill bars. (7) stats.rs:141 — the locale-pack loader is kept single-flight by a cooldown rather than an in-flight flag, so a slow load can be joined by a second worker (duplicate parse, same result).

Remediation decision: Track as debt; when `ui/comparison.rs` and `locales/` are next opened, do (3) and (2)/(4) together, since translating `KEPT_GEAR_HEADLINE` is precisely what invalidates `from_label`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W204

- Task: T209 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/optimize_flow.rs:292`.
- Dependencies: story entry gate.

Claim: Two different Nexus log channel names are used inside single files. optimize_flow.rs logs under `"GW2BuildOpt"` at 152, 712, 760 and under `"GW2 Build Optimizer"` at 292, 359, 479; stats.rs logs under `"GW2BuildOpt"` at 213, 351, 411, 477 and `"GW2 Build Optimizer"` at 330, 455. Repo-wide, `"GW2 Build Optimizer"` is the registered addon name (lib.rs:26) while `"GW2BuildOpt"` is used throughout main_view.

Remediation decision: Pick one — the registered `"GW2 Build Optimizer"` from lib.rs:26 is the obvious canonical choice — put it behind a `const LOG_CHANNEL: &str` in the addon crate root, and use it everywhere.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W205

- Task: T210 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/resolution.rs:13`.
- Dependencies: story entry gate.

Claim: `resolve_selected_build` is a pure pass-through to `resolve_selected_build_inner` (line 17) with an identical signature AND identical visibility — both are `pub(super)` — so the wrapper buys nothing, not even a visibility boundary. grep shows both names are live: mod.rs:818/864/893 call the wrapper, character.rs:156 and stats.rs:338/464 call the inner directly. (The similar pair in character.rs:322/326 is not this: there the inner really is private.)

Remediation decision: Delete the wrapper and point mod.rs's three call sites at `resolve_selected_build_inner`, or rename the inner to `resolve_selected_build` and drop the `_inner` suffix everywhere.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W206

- Task: T211 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/tabs/mod.rs:3`.
- Dependencies: story entry gate.

Claim: The module doc enumerates five tabs (new_build, improve, kitchen, settings, saveload) but the module declares eight (lines 14-21 add `about`, `news`, `radio`). The three newest tabs — About/feedback wizard, News desk, Radio player — are the largest files in the directory and are missing from the index comment.

Remediation decision: Add `about`, `news`, and `radio` bullets to the list (or drop the list and let the `pub(super) mod` lines speak).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W207

- Task: T212 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/tabs/radio.rs:776`.
- Dependencies: story entry gate.

Claim: radio.rs imports the i18n helpers `t` and `tf` at line 16 and then shadows both with numeric locals in functions that also draw translated labels: `row_controls` binds `let tf = ui.frame_count() as u32` (776) five lines after calling `t("radio.play")`, and `now_playing_marquee` names its frame-counter parameter `t: u32` (1433). Inside those scopes any new `tf("fmt…")`/`t("…")` call fails to compile with a confusing 'expected function, found u32' rather than doing the obvious thing.

Remediation decision: Rename the locals to `frame`/`frames` (the sibling `toggle_favorite` already uses `now_frames`).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W208

- Task: T213 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/main_view/tabs/settings.rs:70`.
- Dependencies: story entry gate.

Claim: settings.rs paints status/legend text with 15 raw RGBA literals (pure green `[0.0,1.0,0.0,1.0]`, orange `[1.0,0.5,0.0,1.0]`, red `[1.0,0.3,0.3,1.0]`, yellow `[1.0,1.0,0.0,1.0]`, `[0.6,0.8,1.0,1.0]` header, etc.) while the crate has a runtime palette (`theme::pal()`, presets + custom theme, see theme-system memory) and named semantic colours `theme::OPTIMIZED`/`WARN`/`ERR` that the other tabs in this batch use for the same meanings (e.g. about.rs status_view, saveload.rs:88-90).

Remediation decision: Replace each literal with the matching token (`theme::OPTIMIZED`, `theme::WARN`, `theme::ERR`, `theme::pal().gold/muted`), adding a `theme::INFO` if a blue accent is really wanted.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W209

- Task: T214 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/addon/src/ui/radar_chart.rs:68`.
- Dependencies: story entry gate.

Claim: The trailing `.min(avail_w)` is a no-op: `avail_w.min(x)` is already `<= avail_w`, so clamping it to `avail_w` again cannot change the result for any finite input.

Remediation decision: `let size = avail_w.min((avail_h - 48.0).max(176.0));`

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W210

- Task: T215 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/core/src/config.rs:586`.
- Dependencies: story entry gate.

Claim: Deliberate shortcut in `sibling_bak`: a single `config.json.bak` slot that each unparseable-config recovery overwrites, rather than timestamped history. Stated ceiling - "timestamp rotation if history is needed" - is not reached: the only writer is `backup_unparseable_config` (config.rs:592), reached once per failed load.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W211

- Task: T216 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/core/src/config.rs:841`.
- Dependencies: W009.

Claim: Deliberate shortcut in `AppConfig::save`: the staging file is not fsynced before the rename, so a save survives a process crash but not a power cut mid-write. The stated ceiling is explicit - revisit if config saves move off the overlay render thread, where an fsync is a visible frame hitch. Note that config saves have already been handed to a background worker (see the `staging_path` doc at config.rs:865-870 and `MESSAGE_WRITES`-style workers in the addon), so the stated condition is closer than the comment implies.

Remediation decision: Confirm whether `AppConfig::save` still runs on the render thread; if not, add `File::sync_all` in the staging write the way `storage.rs::write_durably` does.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W212

- Task: T217 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/core/src/storage.rs:249`.
- Dependencies: story entry gate.

Claim: Deliberate shortcut in `publish_by_rename`, the fallback for volumes that cannot hard-link (exFAT, some network shares): the existence check before the rename is a check, not a claim, so two processes racing the same build name on such a volume can end with the later save winning instead of erroring. Stated ceiling - "Upgrade path is a real lock file if anyone ever reports it" - not reached.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W213

- Task: T218 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/gw2api/src/cache.rs:80`.
- Dependencies: story entry gate.

Claim: The `is_stale` doc credits a serde attribute that appears nowhere in the file: `grep -rn 'deny_unknown_fields' crates/gw2api/src` returns only this comment line. The mechanism is actually a locally declared `struct Meta { build: u32 }` (cache.rs:87-90) relying on serde's default tolerance of unknown fields - there is no attribute to configure, and none was removed.

Remediation decision: Reword to describe what is there: a minimal `Meta` struct that ignores unknown fields by default, read through a `BufReader` so the 50 MB items cache is never materialized as a `String`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W214

- Task: T219 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/gw2api/src/graphics.rs:166`.
- Dependencies: story entry gate.

Claim: Deliberate shortcut in `download_missing`: a fixed 6-way split of the pending icon list (`pending.chunks((total / 6).max(1))`) with no rate limiting, on the grounds that render.guildwars2.com is a CDN rather than the game API. No stated ceiling has been hit; `fetch_bytes` (client.rs:718-720) documents the matching decision to skip the token bucket for icons.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W215

- Task: T220 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/gw2api/src/localize.rs:195`.
- Dependencies: story entry gate.

Claim: `ids_from_cache_str` adds nothing: it forwards verbatim to `ids_from_cache_u32` (localize.rs:186), which is name-agnostic anyway - it just clones the `id` field out of each cached row as a `serde_json::Value`. It has one production caller (`fetch_str`, localize.rs:245) and one test caller (localize.rs:288).

Remediation decision: Delete the alias, call `ids_from_cache_u32` from `fetch_str` and the test, and rename it to something type-neutral such as `ids_from_cache`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W216

- Task: T221 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/examples/nudge_druid_check.rs:58`.
- Dependencies: story entry gate.

Claim: Three leftovers in this diagnostic example. (1) `on_progress` is defined and then immediately discarded with `let _ = on_progress;` — never passed anywhere; the run uses the `noop` closure defined right below (which is not a no-op, it prints). Its format string also hardcodes `0.0` for the elapsed-seconds field, so it would print `[  0.0s]` forever if it were used. (2) nudge_druid_check.rs:32-33 binds a match arm as `mode => { let _ = mode; ... }`, discarding the binding it just made. (3) nudge_druid_check.rs:140 declares `let forced_names: [(&str, u32); 3]` where the `u32` is `1` in all three entries and is discarded at the use site (`for (name, _) in forced_names`, line 143).

Remediation decision: Delete the `on_progress` closure and its `let _ =`; change the match arm to `_ => OptimizationWeights { ... }`; change `forced_names` to `[&str; 3]` and iterate `for name in forced_names`. Rename `noop` to `progress` since it prints.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W217

- Task: T222 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/data/cleanse_sources.rs:92`.
- Dependencies: story entry gate.

Claim: Four `CleanseSource` fields are deserialized from `data/cleanse_sources.json` and never read by any code, including the `examples/cleanse_registry_check.rs` audit tool: `mechanism` (line 92), `trigger` (line 102), `slot` (line 90) and `evidence` (line 112). Verified with `grep -rn --include=*.rs '\.mechanism\|\.trigger\b\|\.evidence\b' crates server examples` — the only `.trigger` hit is `proc_spec.trigger` on an unrelated struct in rotation/wvw_timeline.rs:1355, and there are no `.mechanism`/`.evidence` hits at all. (`notes` and `ally_count` are read, but only by the loader's own validation at lines 265 and 267.)

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W218

- Task: T223 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/data/slot_budgets.rs:155`.
- Dependencies: story entry gate.

Claim: The only two `#[allow(...)]` suppressions in this entire batch (`grep -rn '#\[allow' crates/optimizer/src/data/*.rs` returns exactly these two), both on `SlotBudgetFile` fields `rarity: String` (line 156) and `level: i32` (line 158). They are parsed from `data/slot_budgets/level80_ascended.json` and never read — the suppression is what keeps rustc quiet about it.

Remediation decision: Validate them instead of suppressing — assert `rarity == "Ascended" && level == 80` in `validate_entries` and drop both `#[allow(dead_code)]` attributes; the fields then earn their place and the file's provenance is checked at load.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W219

- Task: T224 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/gamedb.rs:687`.
- Dependencies: story entry gate.

Claim: `empty_for_tests` is a 30-line test constructor compiled unconditionally into the shipping cdylib — it is `pub` with no `#[cfg]` gate. Every one of its ~40 call sites is test code: crates/addon/src/chat_links.rs:436/485/508/529, crates/addon/src/state.rs:1898, crates/addon/src/ui/comparison.rs:1232, and the optimizer's own `#[cfg(test)]` modules. It is ungated because `#[cfg(test)]` does not cross the crate boundary into crates/addon's tests, which is a real constraint, not an oversight.

Remediation decision: Gate synthetic database constructor through test-support and addon dev dependency, verifying release feature resolution.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W220

- Task: T225 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/gemini.rs:24`.
- Dependencies: story entry gate.

Claim: GEMINI_API_BASE (line 24) and GEMINI_MODELS_URL (line 25) are two separately-named constants holding the identical string "https://generativelanguage.googleapis.com/v1beta/models". Both are live — GEMINI_API_BASE is used in stream_url (line 66) and GEMINI_MODELS_URL in models_request (line 93) — so this is duplication, not dead code.

Remediation decision: Keep one const (e.g. GEMINI_MODELS_URL = GEMINI_API_BASE) or define the base once and derive the models URL from it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W221

- Task: T226 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/gemini.rs:266`.
- Dependencies: story entry gate.

Claim: Declared deferral: RateReserve here is a local twin of llm::openai_compat::RateReserve, kept separate because the two guards are typed on different RateTracker types. The stated ceiling is 'if a fourth tracker ever appears' — there are currently three (gemini.rs RateTracker plus llm::rate::RateTracker used by the openai-compatible providers), so the ceiling is not yet hit. This is the only `ponytail:` marker in the batch (grep over all four files).

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W222

- Task: T227 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/gemini_tools.rs:93`.
- Dependencies: story entry gate.

Claim: A pure pass-through wrapper whose own comment states it exists only for compile compatibility: 'Kept as a thin wrapper so existing call sites and tests continue to compile unchanged.' The body is a single `db.itemstat_by_name(needle)`. There are three production call sites (874, 946, 1521) and four test sites (2283, 2297, 2305, 2469) — all in this one file, all of which could call db.itemstat_by_name directly.

Remediation decision: Inline db.itemstat_by_name at the seven call sites and delete the wrapper; the four determinism tests belong next to GameDb::itemstat_by_name.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W223

- Task: T228 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/anthropic.rs:336`.
- Dependencies: story entry gate.

Claim: send_messages's `system` parameter and MessagesRequest.system (anthropic.rs:59) are never populated: grep 'send_messages(' -> the two call sites at anthropic.rs:633 and :675 both pass None. The module doc (line 6) advertises 'System prompt is a top-level `system` field' as a key difference, but the prompt is always sent as the first user message.

Remediation decision: Drop the parameter and the request field (or actually route the system portion of the prompt through it and keep both).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W224

- Task: T229 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/body.rs:17`.
- Dependencies: W136.

Claim: `MAX_COMPLETION_TOKENS` is 65_536 (openai_compat.rs:55, whose own doc says 'Raised from 16_384'). body.rs:17-18 still sizes the 8 MiB cap against 16_384, and anthropic.rs:34 still says 'Deliberately not the openai-compat family's 16_384'. The arithmetic still holds at 65_536 x 20 bytes = 1.3 MiB, so the cap is not wrong, only the justification.

Remediation decision: Consolidate with W136; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W226

- Task: T230 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/openai_compat.rs:74`.
- Dependencies: story entry gate.

Claim: Present-tense 'Anthropic and Gemini leak a slot' describes a bug that is fixed: AnthropicClient::send_messages holds a RateReserve (anthropic.rs:350) and crate::gemini has its own RateReserve twin (crates/optimizer/src/gemini.rs:270-292) around read_gemini_stream.

Remediation decision: Rewrite as past tense: 'Anthropic and Gemini each leaked a slot on mid-stream failure (GLM F16) until they adopted this guard.'

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W227

- Task: T231 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/rate.rs:173`.
- Dependencies: story entry gate.

Claim: `persist_usage` drops every write/rename error and the three `with_persistence` loaders drop read/parse errors via `.ok().and_then(.. .ok())` (openai.rs:55-57, openrouter.rs:69-71, anthropic.rs:312-314). rate.rs:8-14 (Claude F37) makes RPM enforcement depend on this file surviving between `create_client` calls, so a persistently failing write (read-only addons dir, AV lock) silently turns the RPM limit off; a corrupt file silently resets counters. The optimizer crate has no logger (Cargo.toml has no `log` dep), so there is currently no channel to report it, which is why this is S4 rather than S3.

Remediation decision: Return rate persistence failures to a bounded diagnostic surface without exposing provider keys or response bodies.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W228

- Task: T232 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/response_cache.rs:52`.
- Dependencies: story entry gate.

Claim: Deliberate O(cap) eviction scan on insert; cap is 64 at every construction site (openai.rs:41/70, openrouter.rs:55/84, anthropic.rs:288/327). Ceiling not hit; ledger entry only.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W229

- Task: T233 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/response_cache.rs:52`.
- Dependencies: W228.

Claim: Deliberate O(cap) min_by_key scan on insert with a stated ceiling (a BTreeMap 'if the cap ever grows hot'); cap is 64 and the whole cache has no production caller (see the generate_cached finding), so the ceiling is nowhere near reached.

Remediation decision: Consolidate with W228; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W230

- Task: T234 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/sse.rs:9`.
- Dependencies: story entry gate.

Claim: Anthropic streaming exists as `read_anthropic_stream` (anthropic.rs:164-279) and uses none of these primitives except the index cap; it is a separate reader, not an adapter over `read_stream`/`apply_chunk`. Likewise openai_compat.rs:75-76 says 'Anthropic and Gemini leak a slot on mid-stream failure (GLM F16)' while anthropic.rs:350 now holds a `RateReserve` and crate::gemini has its own guard (gemini.rs:280-292).

Remediation decision: Reword sse.rs:9-10 to 'Anthropic streams a different event shape; see `anthropic::read_anthropic_stream`' and change openai_compat.rs:75-76 to past tense ('used to leak').

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W232

- Task: T235 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/tools.rs:2`.
- Dependencies: story entry gate.

Claim: `tool_definitions()` is live (crates/addon/src/ui/main_view/chat_flow.rs:165, optimize_flow.rs:668) but the provider-neutral `ToolDefinition` is still derived from `gemini_tools::tool_declarations()` and, for the Gemini provider, converted straight back by `to_gemini_tools` (llm/gemini.rs:89-100), which then has to strip schema keywords Gemini rejects. The module doc is written as a migration note ('remains unchanged') for a migration whose end state, neutral definitions as the source of truth, never arrived.

Remediation decision: Have `gemini_tools` (or a new `tools` module) define `Vec<ToolDefinition>` directly and derive Gemini's `FunctionDeclaration` from it in `to_gemini_tools`; rewrite the module doc to describe the current design.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W233

- Task: T236 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/llm/tools.rs:3`.
- Dependencies: story entry gate.

Claim: Two migration-era module docs no longer match the code. tools.rs:3-4 says gemini_tools::execute_tool 'remains unchanged' and takes name + args; its actual signature is execute_tool(name: &str, args: &Value, ctx: &ToolContext) (gemini_tools.rs:141). sse.rs:9-10 says 'Anthropic's event-based SSE will need an adapter on top of these primitives'; anthropic.rs:164 instead ships its own read_anthropic_stream that reuses only MAX_TOOL_CALL_INDEX/slot_index_rejected.

Remediation decision: Trim tools.rs to 'Converts gemini_tools::tool_declarations() into provider-neutral ToolDefinitions' and change sse.rs to state that Anthropic has a separate reader sharing the index cap.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W234

- Task: T237 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/parser_consistency_tests.rs:25`.
- Dependencies: story entry gate.

Claim: Broken intra-doc link: there is no `Expectation` type in this file or anywhere in crates/optimizer/src (grep -rn 'Expectation' crates/optimizer/src returns only this line). The construct the sentence means is the `Case` struct at parser_consistency_tests.rs:81-86 and its `expected: Vec<FactClass>` field, described at lines 77-80.

Remediation decision: Change to `(see the `expected` field on [`Case`])`, or rename `Case` to `Expectation` if that was the intent.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W235

- Task: T238 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/prompts.rs:313`.
- Dependencies: story entry gate.

Claim: The chat prompt ends two consecutive paragraphs with the identical clause 'explanation: 2-4 sentences in {reply_language}.' — line 311 and line 313 — and the JSON schema at line 362 states the same requirement a third time, on top of the standalone instruction at line 302 ('Write the "explanation" field in {reply_language}'). The three prompt builders are snapshot-tested (snapshot_chat_refinement_prompt_with_tools), so the duplicate is pinned in the expected literal rather than caught by it.

Remediation decision: Drop the trailing clause from line 313 (line 311 already states it, and the JSON schema restates it) and update the snapshot literal in the same change.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W236

- Task: T239 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/rotation/simulator.rs:1048`.
- Dependencies: story entry gate.

Claim: The `"Might"` alternative in `boon_value`'s match is unreachable: both call sites special-case Might before calling it. `into_result` filters Might out of the boon_equivalents fold at simulator.rs:974 (`.filter(|(name, _)| !name.eq_ignore_ascii_case("Might"))`) and adds `might_stacks_avg / 25.0` separately at 979; `skill_cast_value` branches at 1134 (`if buff.eq_ignore_ascii_case("Might") { (*stacks as f64 / 25.0).min(1.0) } else { boon_value(buff) }`). `grep -n boon_value crates/optimizer/src/rotation/simulator.rs` shows only those two callers plus the definition.

Remediation decision: Drop `"Might"` from the arm and reword the doc to say Might is handled entirely by the callers, not scaled on top of this function.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W237

- Task: T240 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:3`.
- Dependencies: story entry gate.

Claim: The module doc calls `simulator.rs` 'the legacy rotation simulator'. In this project 'legacy' has a specific documented meaning — the third optimiser fallback tier (`engine::optimize()` + `enrich_with_gemini()`). `simulator.rs` is not that tier: it is the live gate and flow simulator, called from engine.rs:1374 and engine.rs:1421 (both inside the current pipeline), from crates/addon/src/ui/main_view/optimization.rs:629, and from gemini_tools.rs:1541.

Remediation decision: Reword to 'The DPCT rotation simulator answers ...' or 'The dummy-target simulator answers ...'. Keep the contrast the paragraph is drawing — it is a good one — without borrowing a term that means something else here.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W238

- Task: T241 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/rotation/wvw_timeline.rs:417`.
- Dependencies: story entry gate.

Claim: The only `// ponytail:` marker in the batch — `grep -rn ponytail crates/optimizer/src/rotation/` returns this single line. It documents that a scenario with no outcome target (`dummy_hp` returns None for Support/Commander/Staller and for Squad tiers other than Harasser, combat_model.rs:235-248) is modelled by setting `enemy_health` to `f64::INFINITY`, so `record_damage`'s `self.enemy_health <= 0.0` guard never trips and `target_reached` stays false.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W239

- Task: T242 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/scoring.rs:77`.
- Dependencies: story entry gate.

Claim: Two public names for one value, with the migration note pointing the wrong way. `WEIGHT_BUDGET` is labelled the legacy alias, yet it is the one the shipping UI uses (crates/addon/src/ui/radar_chart.rs:208) and the one the regression suite pins (tests/scoring_regression.rs:281, :292; tests/objective_profiles_integration.rs:344, :621-624). `DEFAULT_WEIGHT_BUDGET` has zero references outside scoring.rs (used internally at L166, L202, L457). No caller anywhere reads `profile.weight_budget` for this purpose — the only place it is stored is the dead ObjectiveScorer.

Remediation decision: Pick one name. Since `WEIGHT_BUDGET` is what the addon and the pinning tests use, delete `DEFAULT_WEIGHT_BUDGET`, point the three internal uses at `WEIGHT_BUDGET`, and replace the doc with the truth: it is the calibrated 2.0 budget the UI enforces. Separately decide whether the serde compat aliases are still needed for saved builds in the field.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W240

- Task: T243 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/search.rs:47`.
- Dependencies: story entry gate.

Claim: search.rs defines private TRINKET_SLOTS (47-54), WEAPON_SLOTS (55) and ARMOR_SLOTS (56-63) whose contents are identical to the pub consts validation.rs exports as TRINKET_SLOTS (28-35), WEAPON_SET1_SLOTS (39) and ARMOR_SLOTS (18-25); search_v2.rs already imports the validation.rs versions (line 20-21).

Remediation decision: Import the validation.rs consts in search.rs and delete the local copies.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W241

- Task: T244 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/search_v2.rs:726`.
- Dependencies: story entry gate.

Claim: The 'Funnel diagnostic' comment (726-729) sits directly above `if !beam[0].report.viability.is_viable {`, the seed-repair block, not above the fn_* counters it describes (772-780). Its third line is also garbled ('admission was (`generated=31977 admitted=1500`)').

Remediation decision: Move the comment to sit above `let mut fn_generated` and finish the sentence ('admission was the bottleneck: ...').

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W242

- Task: T245 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/search_v2.rs:1233`.
- Dependencies: story entry gate.

Claim: Operator docs are numbered inconsistently: swap_gear_prefix is 'Operator 1' (1026), swap_gear_groups 'Operator 2' (1053), swap_slot_prefix 'Operator 3' (1101), then swap_rune is again 'Operator 2' (1233), swap_sigil_slots again 'Operator 3' (1252), swap_relic 'Operator 4', swap_utility_skills 'Operator 5', and the remaining six operators are unnumbered. generate_neighbors' doc (314) says there are six.

Remediation decision: Drop the numbers from the docs (the generate_neighbor_groups push order is the authoritative list).

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W243

- Task: T246 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy.rs:31`.
- Dependencies: story entry gate.

Claim: StatType::from_api re-encodes the GW2 attribute alias table (ConditionDuration|Expertise, BoonDuration|Concentration, CritDamage|Ferocity, Healing|HealingPower) that already lives in stats.rs StatBlock::add/get (36-38, 64-66) and again in engine.rs:1022-1024; stat_type_from_display_name (synergy.rs:1058) adds a fourth, display-string variant for rune bonus text.

Remediation decision: Add one `canonical_attribute(&str) -> Option<&'static str>` in stats.rs and derive StatType::from_api and the engine.rs match from it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W244

- Task: T247 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy.rs:46`.
- Dependencies: story entry gate.

Claim: StatType::label has no callers. `grep -rn '\.label()'` across the files that import synergy::StatType (upgrade_graph.rs, data/normalized_effects.rs, synergy_pipeline.rs) finds only `ctx.game_mode.label()`; upgrade_graph.rs:507 defines its own stat_key() mapping instead.

Remediation decision: Delete it, or have upgrade_graph::stat_key reuse it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W245

- Task: T248 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy.rs:708`.
- Dependencies: story entry gate.

Claim: compute_marginal_synergy's `new_id: Option<&ComponentId>` is Some at every call site: `grep -rn -A4 'compute_marginal_synergy(' crates server | grep None` returns nothing (synergy_pipeline.rs ×8, synergy.rs tests ×3 all pass Some(&...)). The doc's 'legacy/test cases may pass None' fallback branch (`unwrap_or_else(|| existing.clone())` at 720) is dead.

Remediation decision: Take `new_id: &ComponentId` and drop the closure fallback.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W246

- Task: T249 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:377`.
- Dependencies: story entry gate.

Claim: The 27 cap can never bind: each per-spec config list is truncated to 3 (line 366 `configs.truncate(3)`), spec_ids has at most 3 entries (search_spec_combos yields elite+2 core or 3 core), so the cross-product is at most 3^3 = 27. `combo_count` is computed only to feed this no-op min, and `cross_products.into_iter().take(combo_limit)` takes everything.

Remediation decision: Delete combo_count/combo_limit and the take(), or make the per-spec top-N a named const and derive the cap from it.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W247

- Task: T250 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:486`.
- Dependencies: story entry gate.

Claim: The greedy argmax loop (best_score/best_item/best_effects/best_links; for each item: extract effects, sum score_normalized_effect, compute_marginal_synergy, keep if total > best) is copy-pasted four times: select_rune (486-513), select_sigils (549-587), select_relic (610-635) and pick_best_skill (1230-1247), differing only in the extractor and ComponentId variant.

Remediation decision: One generic `best_by_synergy<T>(items, extract: impl Fn(&T)->Vec<NormalizedEffect>, id: impl Fn(&T)->ComponentId, weights, accumulated)` used by all four.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W248

- Task: T251 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:1384`.
- Dependencies: W176.

Claim: compute_candidate_stats adds trait_stats to full_stats field by field over nine lines (1384-1392) while StatBlock implements AddAssign<&StatBlock> over exactly those nine fields (stats.rs:87-99), and the same function uses `full_stats += &gear_stats;` seven lines earlier (1377).

Remediation decision: Consolidate with W176; verify this entry's distinct claim before closing.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W249

- Task: T252 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/synergy_pipeline.rs:1409`.
- Dependencies: story entry gate.

Claim: build_synergy_result takes `_weights` and never uses it (the underscore silences the lint). Both callers (line 214 and the diagnostic test at 2354) pass weights through for nothing.

Remediation decision: Remove the parameter from the signature and both call sites.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W250

- Task: T253 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/upgrade_graph.rs:111`.
- Dependencies: story entry gate.

Claim: Two unused public accessors. `UpgradeGraph::nodes` — grep -rn '.nodes()' crates server returns no hit outside upgrade_graph.rs; the graph's live consumers use `get`, `search`, `synergies` and `format_catalog_slice` (context.rs:364, gemini_tools.rs:788-828). `LandWeaponBudget::slot_count` (crates/optimizer/src/weapon_budget.rs:78) — grep for 'slot_count()' returns nothing outside weapon_budget.rs; engine.rs:1563/1565 use `is_two_handed()` and `slots()` instead. Both are exercised only by their own tests (upgrade_graph.rs test module; weapon_budget.rs:343-350).

Remediation decision: Delete both, and the `slot_count` assertions inside `slot_count_matches_the_slots_listed` (weapon_budget.rs:343-347), keeping the `is_two_handed` assertions there.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W251

- Task: T254 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/src/validation.rs:810`.
- Dependencies: story entry gate.

Claim: `validate_weapon_set` handles the main hand (validation.rs:778-807) and the off hand (validation.rs:810-837) with two ~29-line blocks that are line-for-line identical apart from the variable name (`mh`/`oh`) and the field assigned (`set.main_hand`/`set.off_hand`) — same `find_weapon` call, same `land_usable` check, same `WeaponNotAvailable` reject with the same two `detail` strings, same else-branch reject. The only asymmetry is a comment present on one copy and not the other (validation.rs:793-794 explains why the canonical name is stored; the off-hand copy has no such note). A second instance sits in the same file: `find_item_by_name` repeats an identical five-line 'push fuzzy-matched warning, return Some(ValidatedItem { id, name })' block three times, at validation.rs:1530-1539, 1552-1561 and 1578-1587.

Remediation decision: Extract `fn validate_one_hand(name: &str, prof: &Profession, label: &str, result: &mut ValidatedBuild) -> Option<String>` and call it twice, assigning to `set.main_hand` / `set.off_hand`. In `find_item_by_name`, extract a small `fn fuzzy_hit(item: &Item, item_type: &str, name: &str, result: &mut ValidatedBuild) -> Option<ValidatedItem>` for the repeated warn-and-return block.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W252

- Task: T255 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/tests/live_llm.rs:200`.
- Dependencies: story entry gate.

Claim: Step 6 of the numbered provider suite has a heading and no test — the next statement is the suite's closing success print. Steps 1-5 (validate_key, generate, generate_cached, generate_with_tools, remaining_quota) all have real bodies at live_llm.rs:83-197. Invalid-key coverage does exist, but as separate `#[ignore]`d per-provider tests (`test_gemini_invalid_key` :215, `test_openai_invalid_key` :245, `test_anthropic_invalid_key` :275, `test_openrouter_invalid_key` :311), not inside `run_provider_tests`.

Remediation decision: Delete the orphan `// 6. Invalid key test` line (the standalone per-provider tests already cover it), and add an OPENROUTER_API_KEY branch to `test_all_providers_canonical_build_smoke` matching the other three.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W253

- Task: T256 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/tests/math_permutations.rs:2`.
- Dependencies: story entry gate.

Claim: A planning-phase gate statement left in the module doc of a shipped test file. The project is feature-complete through S15 with chat and UI long since delivered (crates/addon/src/ui/main_view/chat_flow.rs, tabs/*), so nothing 'waits' on this suite; it is an ordinary regression suite.

Remediation decision: Replace with what the file actually is, e.g. `//! Regression suite for combat-math fact interpretation; one test per permutation.`

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W254

- Task: T257 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/tests/scoring_regression.rs:146`.
- Dependencies: story entry gate.

Claim: Three stale claims in one comment block and one nearby. (1) 'see NOTES in the report' names an artefact that is not in the repo — no NOTES file, no 'the report'; a reader has no way to find the reasoning. (2) 'Three power+condi+heal+roam' says three but enumerates and uses four fixtures: `dragonhunter_power_pve` (:78), `harbinger_condi_pve` (:96), `druid_heal_pve` (:114), `daredevil_wvw_roam` (:131). (3) at scoring_regression.rs:190 the comment 'A custom WvW preset doesn't yet exist' is out of date — WvW objective profiles ship in data/objective_profiles and are reachable via `RoleObjective::to_weights_for(&GameMode::WvW, tier)` (crates/optimizer/src/scenario.rs:337).

Remediation decision: Inline the Tank-Chrono rationale (the following three lines already state it) and drop 'see NOTES in the report'; change 'Three' to 'Four'; at line 190 either switch `daredevil_wvw_roam` to the real WvW profile weights via `to_weights_for` and re-pin, or say why the PvE power preset is deliberately used.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W255

- Task: T258 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `crates/optimizer/tests/upgrade_graph_live.rs:43`.
- Dependencies: story entry gate.

Claim: The comment points the reader at `LIVE_CACHE`, which does not exist: grep -rn --include=*.rs 'LIVE_CACHE' crates server returns this line and nothing else. The test actually resolves its cache through `gw2_api::dev_config::cache_dir()` (upgrade_graph_live.rs:18), which the file's own module doc correctly describes at lines 15-16 as 'The live cache named by dev.cfg (see dev.cfg.example)'. The reference is a fossil of a removed constant, presumably from before the dev.cfg mechanism replaced it.

Remediation decision: Replace the trailing comment with `// Requires the in-game addon's live items cache (see dev.cfg.example)`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W256

- Task: T259 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `docs/architecture.md:7`.
- Dependencies: story entry gate.

Claim: The repository's only architecture document covers just the DLL. Searching it for 'feedback', 'server', 'radio', 'News' or 'theme' returns zero hits, yet the tree contains a second deployable (server/feedback: axum + Postgres + Dockerfile + compose + its own CI workflow, live at feedback.robagentic.tech) plus the radio, news and theme subsystems that CLAUDE.md and README.md both describe as shipped features. The document's own Workspace Layout table and Data Flow diagram stop at the four crates.

Remediation decision: Add a short 'Second deployable: server/feedback' section (crate layout, endpoints, deploy path, pointer to server/feedback/deploy/README.md) and one line each for radio/news/themes in the crates/addon row.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W257

- Task: T260 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `docs/superpowers/plans/2026-08-26-per-slot-gear-implementation.md:155`.
- Dependencies: W188.

Claim: Every checkbox in this plan's seven tasks is still unchecked, but the feature shipped: `GearSlots` appears in 9 source files (core/types.rs, core/feedback/report.rs, optimizer/{engine,referee,search,validation}.rs, addon/{feedback/mod,ui/comparison,ui/main_view/tabs/saveload}.rs) and `slot_prefixes` in 12. The checkbox ledger is therefore unusable as state, and it hides the one item that genuinely is not done - Task 7 'Delete `GearPrefixGroups`/`ValidatedGearGroups` construction paths' (line 222), with GearPrefixGroups still live in 5 files. The sibling plan 2026-08-26-foundational-remediation.md keeps its boxes checked and adds a dated Status section, so the convention exists and this file diverges from it.

Remediation decision: Tick the completed steps and add a dated Status section (as the foundational-remediation plan has) naming Task 7 as the only outstanding item, or archive the plan and move the remaining cleanup into an issue.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W258

- Task: T261 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `server/feedback/Cargo.toml:23`.
- Dependencies: story entry gate.

Claim: Two dependency-manifest inaccuracies. (1) The `trace` feature of tower-http is enabled but unused: `grep -rn 'TraceLayer\|tower_http::trace' server/feedback` returns nothing, and the only tower-http import is `tower_http::limit::RequestBodyLimitLayer` (app.rs:14). (2) `tower` (line 22) is a normal dependency but is only used from test code - `use tower::ServiceExt` appears at admin.rs:459 (inside `#[cfg(test)] mod session_tests`) and tests/api.rs:8, nowhere in shipping paths. Both were copied verbatim from the plan's Cargo.toml block (feedback-server.md:222-223) and never revisited.

Remediation decision: Drop `"trace"` from tower-http's feature list (or add the TraceLayer the feature was meant for) and move `tower` to [dev-dependencies].

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W259

- Task: T262 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `server/feedback/src/ratelimit.rs:20`.
- Dependencies: story entry gate.

Claim: The one `// ponytail:` marker in this batch. Ledger entry only - the stated ceiling ('if p99 ever matters') is nowhere near hit: the limiter is a single Mutex<HashMap> capped at MAP_CAP = 10_000 entries (ratelimit.rs:5) serving a feedback endpoint limited to 10 requests/minute per IP. The lock is only ever held for an in-memory retain/push, never across I/O. The same comment is duplicated in the plan's embedded copy at docs/superpowers/plans/2026-08-24-feedback-server.md:1100.

Remediation decision: Review the documented deliberate limitation against current code; retain with rationale and a concrete reconsideration trigger if still valid. This is not an implemented fix.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W260

- Task: T263 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `server/feedback/src/ratelimit.rs:109`.
- Dependencies: story entry gate.

Claim: Two limiter tests exist twice, character-for-character in their assertions: `limiter_sliding_window` at ratelimit.rs:109 and tests/api.rs:398, and `limiter_with_a_zero_limit_rejects_instead_of_panicking` at ratelimit.rs:120 and tests/api.rs:909. The unit-test copies use `super::*`; the integration copies re-import `gw2bo_feedback::ratelimit::RateLimiter` and `std::time::Duration`. The integration copies add nothing (the type is public either way) and were carried over from the plan's Task 5 block, which put them in tests/api.rs before ratelimit.rs grew its own `mod tests`.

Remediation decision: Delete the two duplicated tests from tests/api.rs and keep the `#[cfg(test)] mod tests` copies next to the implementation.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W261

- Task: T264 [US5]. Status: **planned**.
- Audit: S4, confirmed; location: `server/feedback/tests/api.rs:813`.
- Dependencies: story entry gate.

Claim: Two integration-test names assert the opposite of what they claim, left over from when admin GET marked reports read. This one ends in `_and_marks_read` but its assertions are `assert_eq!(seen[0]["status"], "received", "GET list must leave every row received")` (line 838) before the explicit POST /reports/read; api.rs:540 `admin_get_one_returns_full_row_and_marks_read` likewise asserts `"GET one must not mark read"` (line 556) and `"GET one must leave the row received"` (line 565). api.rs:500 `admin_list_marks_read_and_reply_marks_answered` has the same stale first half.

Remediation decision: Rename to what they assert, e.g. `admin_list_newest_first_and_leaves_rows_received` and `admin_get_one_returns_full_row_without_marking_read`.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W262

- Task: T265 [US5]. Status: **planned**.
- Audit: S3, contested; location: `crates/addon/src/ui/icons.rs:17`.
- Dependencies: story entry gate.

Claim: This module handles a poisoned mutex by silently doing nothing (`if let Ok`) and by silently returning `None` (`GRAPHICS_DIR.lock().ok()?` at line 24, `REQUESTED.lock().ok()?` at line 56). Every other static mutex in this crate uses the documented poison-tolerant pattern instead - `state::lock_state` (state.rs:811), `WorkerRegistry::lock` (state.rs:232), `SerialWriter::lock` (ui/mod.rs:93), `theme::pal` (theme.rs:333) and `clipboard::pending` (clipboard.rs:56) all recover with `unwrap_or_else(|e| e.into_inner())` and say in comments why.

Remediation decision: Use `unwrap_or_else(|e| e.into_inner())` for both statics, matching the pattern the rest of the crate documents.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W263

- Task: T266 [US5]. Status: **planned**.
- Audit: S3, contested; location: `crates/gw2api/src/dev_config.rs:48`.
- Dependencies: story entry gate.

Claim: `dev_config` is declared unconditionally in the shipping library (`crates/gw2api/src/lib.rs:2: pub mod dev_config;` - no `#[cfg(test)]`, no feature gate), yet every caller is a dev artifact: five `crates/optimizer/examples/*.rs` files and two `crates/optimizer/tests/*.rs` files (`grep -rn 'dev_config' crates server`). The module reads a workspace-root `dev.cfg` through a `CARGO_MANIFEST_DIR`-derived constant (dev_config.rs:14) and exposes `cache_dir_or_exit`, which prints to a nonexistent stderr and calls `std::process::exit(2)` - inside the addon that is the game process. I confirmed the current build is not leaking the path: a strings scan of `target/release/gw2_build_optimizer.dll` finds zero occurrences of `dev.cfg` or the repo path, so the linker is stripping the unused module today.

Remediation decision: Gate the module for its real consumers - `#[cfg(any(test, feature = "dev-config"))]` (examples/tests enable the feature) - and drop `cache_dir_or_exit`'s `process::exit` in favour of returning the `Result` that `cache_dir` already provides.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W264

- Task: T267 [US5]. Status: **planned**.
- Audit: S4, contested; location: `crates/addon/src/ui/main_view/tabs/radio.rs:1520`.
- Dependencies: story entry gate.

Claim: The comment justifies an ASCII hyphen by claiming the overlay font cannot render an em dash, but em dashes are rendered as user-facing text elsewhere in the same overlay: chat_flow.rs:106 ("(none yet — talk or run Optimize first)"), settings.rs:115 and 1465 (footer and download overlay), and the en locale strings that about.rs tests assert on-screen ("Not sent — Couldn't reach Choya…", about.rs:985, wizard.rs:1715). Either the comment is stale (the bundled fonts gained the glyph) or those sites are broken; the status-line code itself is fine either way.

Remediation decision: Verify once in-game and either delete the comment (keeping or switching the separator) or, if the claim is true, fix the other sites and move the constraint into fonts.rs docs.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W265

- Task: T268 [US5]. Status: **planned**.
- Audit: S4, contested; location: `crates/optimizer/examples/nudge_druid_check.rs:140`.
- Dependencies: story entry gate.

Claim: The diagnostic block at nudge_druid_check.rs:140-233 synthesises three fake ItemStat rows into a cloned GameDb when the real prefix is absent, with invented multipliers the comment itself calls 'rough profiles' (line 150): Vigilant's as Power 0.8 / Toughness 0.65 / Concentration 0.65, Seraph's and Harrier's as Power 0.6 / Concentration 0.8 / HealingPower 0.5. Real GW2 three-stat templates are 0.35 major / 0.25 minor — the values used by every other fixture in this repo (e.g. grouped_sheet.rs:56-68, itemstat_pool.rs test rows) and by `data::slot_budgets`. It then prints `intent=` and `exec=` rank components computed from those rows.

Remediation decision: Either use real 0.35/0.25 multipliers for the injected rows (so the numbers are on the same scale as the rest of the output), or skip the forced-prefix block entirely when the prefix is not in the live cache and print why, rather than inventing a row.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W266

- Task: T269 [US5]. Status: **planned**.
- Audit: S4, contested; location: `crates/optimizer/src/llm/anthropic.rs:797`.
- Dependencies: story entry gate.

Claim: validate_key_does_not_post_messages asserts on the source text of anthropic.rs itself (include_str! of the file, split on 'fn validate_key(' / 'fn validate_key_detailed' / 'fn generate('). It passes or fails on token strings rather than behaviour: renaming models_request or reordering the impl breaks it, and a comment containing '.post(' would fail it while an actual POST routed through a helper would not be caught.

Remediation decision: Replace with the ScriptedServer pattern already used in openai_compat.rs tests: point ANTHROPIC_API_BASE at a loopback server, call validate_key/validate_key_detailed, and assert the recorded request is GET /models.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

### W267

- Task: T270 [US5]. Status: **planned**.
- Audit: S4, contested; location: `crates/optimizer/src/search_v2.rs:1221`.
- Dependencies: story entry gate.

Claim: normalized_prefix_name (strip non-alphanumerics, lowercase, drop trailing 's') is the fourth prefix-name stemmer in the crate: scoring.rs:948 and scraper.rs:982 use `trim_end_matches("'s").to_ascii_lowercase()`, engine.rs:762 uses `trim_end_matches("'s").trim().to_lowercase()`. Each exists to match the tier tables' unpossessed spellings ('Marauder') against API names ('Marauder's').

Remediation decision: Move one stemmer into text_util (next to normalize_sigil_family) and use it from all four sites.

Verification: Report evidence imported; current symbols and consumers must be checked before implementation.

Acceptance: Verify current evidence, then record tested correction, duplicate closure, refutation, or retained-deliberate rationale/trigger. Cosmetic edits need formatting/reference checks, not redundant tests.

## Excluded report entries

- R001: rate-tracker duplication refuted by the audit; no change planned.
- R002: feedback client comment refuted by the audit; no change planned.
- R003: generic AGENTS plan instruction refuted by the audit; updating its plan pointer is normal Spec Kit setup, not a defect fix.
