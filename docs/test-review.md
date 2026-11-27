# テスト・検証体制レビュー

pathos のテスト・検証体制を、仕様モデル (`spec/`)、形式検証 (Kani / TLA+ / loom)、差分テスト、ミューテーションテスト、REAPER 比較、CI・フック・ドキュメントの観点でレビューした結果。

---

## 検証で実行したコマンドと結果（実測）

すべて `nix develop` 内で実行した（REAPER 比較・Kani・mutants 全体実行は行っていない）。

| 実行したコマンド | 結果 |
|---|---|
| `nix develop -c cargo test -q -p pathos-core --test differential generators_reach_every_rule -- --nocapture` | pass (6.51s)。ただしコマンド×エラー別の内訳で `move-clip \| overlap` が **46**、`move-clip \| out-of-range` が **61**、`move-clip \| unknown-track` が **73** と `MIN_HITS (100)` 未満だった（全体合算で素通り）。`split-clip \| ok` は **102**、`remove-tempo \| ok` は **115**。 |
| `nix develop -c cargo mutants --list -p <pkg>` (全 7 crate) | ミュータント総数: `pathos-core`: 269, `pathos-agent`: 145, `pathos-measure`: **388**, `pathos-playback`: 66, `pathos-render`: 154, `pathos-reaper`: 58, `pathos`: **927** (`src/main.rs` 347, `src/layout.rs` 295, `src/editor.rs` 130, `src/readout.rs` 55, `src/meter.rs` 44, `src/agent.rs` 39, `src/file.rs` 13, `src/theme.rs` 4)。 |
| `nix develop -c cargo test -q -p <pkg>` (`pathos`, `pathos-agent`, `pathos-measure`, `pathos-playback`, `pathos-reaper`, `pathos-render`) | すべて pass。テスト数: `pathos` 66+3, `pathos-agent` 22, `pathos-measure` 9, `pathos-playback` 10+5+1, `pathos-reaper` 7 (+2 ignored), `pathos-render` 23+3。 |
| `nix develop -c cargo test -q -p pathos-core --lib --test history --test schedule` | pass (1.28s): lib 6, `history.rs` 4, `schedule.rs` 3。 |
| `nix develop -c cargo test -q -p pathos-core --test differential core_agrees_with_model_on_boundaries` | pass (0.31s)。 |
| `nix develop -c bash -c 'RUSTFLAGS="${RUSTFLAGS:-} --cfg loom" cargo test -q -p pathos-playback --release --target-dir target/loom'` | pass (6.19s): 実行されたテストは **1 件** (`every_interleaving_keeps_the_invariants`) のみ。 |
| `nix develop -c bash -c '(cd tla && tlc -workers auto -metadir /tmp/pathos-review-tla SnapshotHandoff)'` | pass (3.57s): 19,933 states generated / 10,447 distinct states, No error。 |
| `nix develop -c bash -c 'mkdir -p /tmp/pathos-review-audit && (cd spec && lake env lean --run Audit.lean /tmp/pathos-review-audit) && diff -u spec/theorems.txt /tmp/pathos-review-audit/theorems.txt && diff -u spec/definitions.txt /tmp/pathos-review-audit/definitions.txt'` | pass: `48 theorems, 0 unfinished: []`、差分なし。 |
| `nix develop -c scripts/test-check-escape-hatches.sh` / `scripts/test-reaper-line.sh` | いずれも pass。 |
| `nix develop -c cargo mutants -p pathos-measure --test-package pathos-measure --re "in integrated_loudness" -o /tmp/pathos-review-mutants` | 23 mutants 中 **2 missed**, 21 caught (78s)。missed: `signal.rs:111:15` (`<` → `<=`), `signal.rs:127:74` (`>` → `>=`)。 |

---

## 指摘一覧（重要度の高い順）

各指摘で「実際に確かめた事実」と「推測・潜在リスク」を分けて記している。

---

### 1. [high] `pathos-measure` の BS.1770 相対ゲート・トゥルーピーク枝刈り・トラック単体計測が十分に守られていない（問い 1・2・5 / crate: `pathos-measure`, ADR 6・36）

- **重要度**: `high`
- **場所**:
  - `crates/pathos-measure/src/signal.rs:107-133`, `L166-227`, `L337-365`, `L394-416`
  - `crates/pathos-measure/src/lib.rs:128-148`, `L187-199`, `L305-306`
  - `.github/workflows/mutants.yml:47-50`
- **何が守られていないか**:
  - 【事実】`pathos-measure` は 388 mutants とライブラリ crate 中最多のロジックを持ちながら、`mutants.yml` / `scripts/verify mutants` の対象外で、テストは計 9 件のみ。
  - 【事実】`quiet_blocks_are_gated_out_of_the_integrated_loudness` (`signal.rs:337-365`) は「-20 dB サイン波 4 秒 + 完全無音 4 秒」しかテストしていない。境界にまたがる 400ms ブロックのエネルギー比は最小でも 1/4 (-6.02 dB) なので `-10 LU` 相対ゲートをすべて通過し、無音ブロックは `-70 LUFS` 絶対ゲートで既に落ちる。つまり **BS.1770 の相対ゲート (-10 LU) が 1 ブロックも除外しないテストしか存在しない**。さらに部分的ミューテーション実測で、400ms ちょうど境界 (`signal.rs:111`) とゲート閾値ちょうど境界 (`signal.rs:127`) の 2 件のミュータントが実際に missed になることを確認した。
  - 【事実】`true_peaks` (`signal.rs:166-227`) はブロック最大値×L1 ノルムによる枝刈り (`bound(index) <= peak`) を実装しているが、枝刈りなしのナイーブ計算と一致する検証や、右チャンネルのみピーク・負のピーク・`TAPS=32` ブロック境界の検証がない。
  - 【事実】`lib.rs:306` のトゥルーピーク検証が `assert!(peak <= 0.1 && peak > -12.0, "{peak}")` と **12.1 dB 幅**で極めて緩く、またテンポ変化時の `bars()` 分割 (`lib.rs:187-199`)、既存ソロトラックがあるときの `alone()`、ちょうど 600 秒境界のテストがない。
  - 【事実】`alone()` (`lib.rs:128-148`) は `master_volume` を戻さずに各トラックを計測しており、`master_volume` が 1000 以外のケースのテストがない（ADR 36 の "as it would sound alone at its volume and pan" との整合が未検証）。
- **それで見逃される具体的な失敗の例**:
  - `signal.rs:131` の相対ゲート `- 10.0` を `- 20.0` に書き換えたり 2 パス目自体を消しても、全テストが通る（静かなイントロ付き楽曲の統合ラウドネスが BS.1770 から数 LU ずれる）。
  - `true_peaks` の `bound()` が隣接ブロック計算を 1 ブロック誤って真のピークを枝刈りしても、単調な定常サイン波のテストでは気づけない。
  - `master_volume = 0` や `500` のプロジェクトを AI レビューしたとき、各トラック単体計測値までマスター音量で減衰・無音化される。
- **直し方の案**:
  - `-20 LUFS` 区間と `-35 LUFS` 区間（`-70 LUFS` 以上だが相対ゲート `-30 LUFS` 未満）を混在させた信号で、相対ゲートが `-35 LUFS` 区間を除外することを検証するテストを追加する。
  - `true_peaks` について、ランダム波形に対して枝刈りあり/なしの結果が一致するプロパティテストと、右チャンネルのみピーク・ブロック境界のテストを追加する。
  - `lib.rs:306` の許容幅を理論値周辺（例: `±0.1 dB`）に締め、テンポ変化あり・ソロあり・`master_volume != 1000`・600 秒ちょうど境界のテストを追加し、`pathos-measure` を `mutants.yml` と `scripts/verify mutants` の対象に含める。

---

### 2. [high] `src/main.rs` の状態配線・ファイル切替・提案管理に自動テストがなく、別プロジェクト切替時の stale 判定漏れが素通りする（問い 1 / crate: `pathos`, `pathos-agent`, ADR 22・25・30・35・36）

- **重要度**: `high`
- **場所**:
  - `src/main.rs:92-100`, `L140-269`, `L612-620`, `L852-909`, `L1356-1368`
  - `crates/pathos-agent/src/proposal.rs:210-249`
- **何が守られていないか**:
  - 【事実】`src/main.rs` (1,462 行・347 mutants) にはテストが 1 件もない。`docs/verification.md:92` は "No test draws the window" と述べているが、`main.rs` には描画だけでなく、キー割り当て (`key`, `handle_proposal`, `handle_file`, `handle_transport`)、未保存終了確認 (`may_close`)、ファイル切替 (`replace`)、エージェントソケット受信 (`listen`)、スコア公開 (`publish`) が同居しており、いずれもテスト対象外。
  - 【事実】`Proposals::refresh` (`proposal.rs:239-249`) は `proposal.base == document.edits().len()` だけで鮮度判定している。そして `Pathos::replace` (`src/main.rs:92-100`) は別ファイルを開いた際に `self.proposals` を無効化せず、`self.looping` や `self.notice` もリセットしない。
  - 【事実】`Proposals::apply` (`proposal.rs:224-229`) は複数コマンド適用中に途中でエラーが出てもロールバックせず、直後の L228 で無条件に `Status::Applied` を記録する。
  - 【事実】`meter_view` (`src/main.rs:612-620`) のホールドマーク色判定は、中央揃えのため 1px 左にずらした `mark.x - rect.x` をゾーン境界 (`high`, `full`) と比較しているため、境界直上のピーク色が 1px ずれる挙動もテストで守られていない。
- **それで見逃される具体的な失敗の例**:
  - 編集回数 5 回のプロジェクト A でエージェントが提案を出し、ユーザーが編集回数 5 回の別プロジェクト B を開いた場合、A 向けの提案が `Stale` にならず B 上でそのまま表示・適用可能になる。さらに途中コマンドで拒否されると B が半端に書き換わったうえ `Status::Applied` になる。
  - 別ファイルを開いても前プロジェクトの `looping` 範囲が残り、意図しない範囲がループ再生される。
  - キーバインド (`Ctrl+Z` / `Enter` / `Esc` / `L` / `K`) の優先順位や修飾キー条件を壊しても CI は気づけない。
- **直し方の案**:
  - `Pathos` の非描画ロジック（ファイル切替時の状態リセット、キー入力のルーティング、提案・ループ・メトロノーム・トランスポート状態の調停）を `gpui` 非依存のコントローラ層へ切り出して単体テストする（`tests/architecture.rs` のレイヤー構造とも整合）。
  - `Proposals` にドキュメント識別子（または履歴の同一性チェック・明示的な `invalidate_all()`）を持たせ、`apply` 失敗時の挙動をテストする。

---

### 3. [high] `src/layout.rs::tick_at` が ADR 32 の丸め規則（`TempoMap::sample_of`）と食い違っており、テストが見逃している（問い 1・5 / crate: `pathos`, ADR 27・32）

- **重要度**: `high`
- **場所**:
  - `src/layout.rs:366-379`, `L959-975`
  - `docs/adr/0027-put-the-transport-a-readout-and-the-master-in-a-top-bar-and-the-keys-in-a-footer.md:22-23`
  - `docs/adr/0032-round-a-tick-to-the-nearest-sample.md:39-43`
- **何が守られていないか**:
  - 【事実】ADR 27 は "The readout must agree with the playhead line and with the schedule's rounding of ticks to frames" と定め、ADR 32 はティック→サンプル変換を最近接丸め (`TempoMap::sample_of`) に変更した。
  - 【事実】しかし `src/layout.rs:368` は `tempo.sample_of(...)` ではなく `tempo.micros(tick) * u128::from(sample_rate.get()) / 1_000_000`（マイクロ秒への切り捨て＋サンプルへの切り捨ての二重切り捨て）を使い続けている。
  - 【事実】テスト `the_playhead_follows_the_tempo` (`src/layout.rs:959-975`) は 48 kHz・120/60 BPM の割り切れる拍境界しか検証していないため、この食い違いを検出できていない。また `src/layout.rs` (295 mutants) はミューテーション対象外。
- **それで見逃される具体的な失敗の例**:
  - 44.1 kHz・120 BPM では `tick = 16` の厳密位置が 367.5 サンプルなので、スケジュール (`sample_of`) ではフレーム 368 に配置されるが、`tick_at` は `micros(16) = 8333` → `8333 * 44100 / 1_000_000 = 367` と計算し、フレーム 367 で既に `tick = 16` に達したと表示する。非整数フレーム境界のテンポ変更や小節跨ぎでリードアウトと再生音が 1 サンプルずれる。
- **直し方の案**:
  - `tick_at` の `at` を `tempo.sample_of(u64::from(sample_rate.get()), tick)` に統一し、「任意の `(project, rate, tick)` で `tick_at(project, rate, sample_of(rate, tick)) >= tick` かつ `sample_of(rate, tick) > 0` のとき `sample_of(rate, tick) - 1` では `< tick`（同一サンプルに丸まる tick を除く）」という整合テストを 44.1 kHz 端数ティック含め追加する。

---

### 4. [high] `generators_reach_every_rule` がエラーをコマンド横断で合算しており、`move-clip` のエラー到達不足とノート付き `split-clip` の希少性を見逃している（問い 2 / crate: `pathos-core`, ADR 4・13・16）

- **重要度**: `high`
- **場所**:
  - `crates/pathos-core/tests/differential.rs:373-491`, `L973-1015`
  - `docs/adr/0016-leave-splitting-a-clip-with-notes-out-of-the-kani-harnesses.md:51-55`
- **何が守られていないか**:
  - 【事実】`generators_reach_every_rule` (`differential.rs:998-1002`) はエラー到達回数を `total(&|_, outcome| outcome == name)` で**全リクエスト種別にわたって合算**している。
  - 【事実】実測の 2,000 トレースで以下を確認した:
    - `move-clip | overlap`: **46 回** (`MIN_HITS = 100` 未満)
    - `move-clip | out-of-range`: **61 回** (`MIN_HITS = 100` 未満)
    - `move-clip | unknown-track`: **73 回** (`MIN_HITS = 100` 未満)
    - `split-clip | ok`: **102 回**（`unknown-clip` 5,716 回に対して成功率は 2% 未満）
    - `add-note | unknown-clip`: **29,634 回**（`ok` 1,564 回の約 19 倍が拒否）
    - `remove-tempo | ok`: **115 回**（`no-tempo-change` 6,249 回に対して 2% 未満）
  - 【事実】ADR 16 は「ノートを含むクリップの `SplitClip` は Kani から除外し、差分テスト等に委ねる」と決定しているが、生成器には `add-note` の直後にそのクリップを分割するチャンク戦略がなく、`split-clip | ok` 自体が 2,000 トレース中 102 回しか起きない（【推測】そのうちノートを含み、かつ分割点を跨ぐノートを短縮するケースはさらにごく少数になる）。
  - 【事実】`track` は `0..3`、`id` は `0..4`、`note_clip` は `0..2` に固定されており (`differential.rs:374-380`)、`notes_then_schedule` (`L455-491`) は単一クリップへの追加しか行わない。
- **それで見逃される具体的な失敗の例**:
  - `SetMasterVolume` の `invalid-volume` や `MoveClip` の `overlap` 検査を誤って壊したり到達不能にしても、`SetTrackVolume` や `AddClip` の同名エラー回数が 100 を超えていれば `generators_reach_every_rule` は通り続ける。
  - `SplitClip` のノート再配置 (`rotate_left`・`partition_point`・オフセット減算、`project.rs:445-459`) は Kani 対象外かつ生成頻度が低いため、複数クリップにノートがあるときの特定配置バグを差分テストが拾い損ねるリスクがある。
- **直し方の案**:
  - `generators_reach_every_rule` で `(request_kind, error)` の到達可能な組ごとに `MIN_HITS` を課す（例: `("move-clip", "overlap")`, `("set-master-volume", "invalid-volume")` 等）。
  - 既存クリップ・ノート・テンポの状態を参照して次のコマンドを選ぶ状態付き生成器（または `clip + notes -> split-clip` / `add-clip -> move-clip` の専用チャンク）を導入し、「ノート付きクリップの分割（特に分割点を跨ぐノートあり）」の到達回数自体を計測・下限チェックする。

---

### 5. [high] 実装とモデルが同じ間違いをしたときの防波堤（手書き期待値テスト・REAPER 比較）が薄い（問い 2・3 / crate: `pathos-core`, `pathos-reaper`, ADR 3・7・12・33・34）

- **重要度**: `high`
- **場所**:
  - `crates/pathos-core/tests/history.rs:1-144`, `crates/pathos-core/tests/schedule.rs:1-106`
  - `.github/workflows/reaper.yml:6-29`, `.githooks/pre-push:17-45`, `scripts/check-reaper-line.sh:14-28`
  - `.github/workflows/spec.yml:56-89`
  - `crates/pathos-reaper/tests/reaper.rs:200-267`
- **何が守られていないか**:
  - 【事実】モデル非依存の手書き期待値テストは `history.rs` に **4 件**、`schedule.rs` に **3 件**の計 7 件しかない。複数テンポ変化を跨ぐ `tick_to_micros` / `sample_of` の具体的数値、`MoveClip` / `DeleteClip` / `RemoveNote` / `SetTempo` / `RemoveTempo`、mute+solo の組み合わせ、エラー優先順位などは手書き期待値テストが存在せず、モデルとの一致テスト (`differential.rs`) にのみ依存している。
  - 【事実】最大の外部オラクルである REAPER 比較は ADR 34 の通り CI では実行されず、PR 本文の自己申告行 (`REAPER comparison: passed <key>`) の照合のみ (`scripts/reaper-key` は REAPER を動かさずにキーを計算可能)。加えて以下の抜け道がある:
    - `.githooks/pre-push:32-35`: Linux 以外（macOS 等）では警告だけで **exit 0 で push を許可**。
    - `.githooks/pre-push:17`: `git merge-base "$local_sha" origin/main` が失敗すると（shallow clone や未 fetch 等）**黙ってスキップし exit 0**。
    - `reaper.yml:6-8`: `pull_request` のみで `push: [main]` では走らず、`Cargo.lock` や `flake.nix` (`nixpkgs-reaper`) 変更も `paths` 対象外。
    - `reaper.rs:200-267`: 生成テストはデフォルト **8 ケース**のみで、`DeleteClip`, `RemoveNote`, `RemoveTempo`, `Undo`, `Redo` を生成せず、`pitch` は `36..96`・テンポは 60〜240 BPM に限定されている。
  - 【事実】`spec.yml:56-89` の定義変更・定理書き換え検知は `::warning::` のみでマージを止めない。また `spec/Audit.lean` 自体の変更は `definitions.txt` にも `docs/verification.md:45` のチェックリスト対象パスにも含まれない。
- **それで見逃される具体的な失敗の例**:
  - AI エージェントが新機能や仕様変更で `spec/Pathos/Project.lean` と `crates/pathos-core` に同じオフバイワン（例: テンポ変化点での区間計算ミス、`MoveClip` の衝突条件、エラー優先順）を入れた場合、差分テストは全 pass し、自己整合的な Lean 定理も通り、定義変更の警告を人間が見落とせばそのままマージされる。
  - macOS から push して `scripts/reaper-key` の出力をそのまま PR に貼った場合、REAPER との突き合わせが一度も行われずに `main` に入る。
- **直し方の案**:
  - `history.rs` / `schedule.rs` の手書き期待値テストを全コマンド・複数テンポ変化・`muted && solo`・境界条件まで拡充し、モデルと同じ誤りを独立に検出できるようにする。
  - `reaper.rs` の生成器に `DeleteClip` / `RemoveNote` / `RemoveTempo` / `Undo` / `Redo` と全ピッチ域 (`0..128`)・広範なテンポを追加する。
  - `docs/verification.md:45` のチェックリスト対象パスに `spec/Audit.lean` と `spec/Main.lean` を追加し、`pre-push` の `merge-base` 失敗時はスキップせずエラーにする。

---

### 6. [high] `scripts/verify` と CI の間に「CI 未実行の検査 (`nix build`)」と「ローカル未実行の検査 (`kani` cover check)」があり、さらに `check-escape-hatches.sh` にバイパス経路がある（問い 2・5 / CI・スクリプト全般）

- **重要度**: `high`
- **場所**:
  - `.github/workflows/rust.yml:26-72`, `.github/workflows/spec.yml:91`
  - `scripts/verify:124`, `L191-200`, `L213`
  - `.github/workflows/kani.yml:79-98`
  - `scripts/check-escape-hatches.sh:39`, `L49`, `L97-115`, `L132-141`, `L155-163`
- **何が守られていないか**:
  - 【事実】`nix build "$root#default" --no-link` は `scripts/verify full` (`scripts/verify:213`) にしかなく、**どの CI ワークフローでも実行されていない**。
  - 【事実】`kani.yml:79-98` にある cover property 充足チェックは `scripts/verify kani` (`scripts/verify:191-200`) にない。また `kani.yml:93` の `awk` は `Checking harness ` 行が 1 行も出力されない形式変化時に無言で exit 0 になる。
  - 【事実】`scripts/check-escape-hatches.sh` には以下のバイパス経路がある:
    1. `.cargo/mutants.toml` の 1 行配列（例: `exclude_re = ["foo"]`）は L156 `*'= ['*` にマッチして `case` を抜けるため L158 `*\"*` のコメント有無チェックを完全に素通りする。
    2. 閾値引き下げ検査 (`L97-141`) は `@@ ` ハンクごとに `removed=()` を初期化し、かつ対応する追加行があるときしか比較しないため、**`PROPTEST_CASES: 10000` の行削除**や**別ハンクへの移動を伴う引き下げ**を検出できない。
    3. Rust のブロックコメント (`/* TODO */`, `/* workaround */`) は L49 が `//*` のみを見ているため素通りする。
    4. `#[ignore = ""]` は L39 が `#\[ignore\]` のみを見るため素通りする。
  - 【事実】`scripts/verify` は `cargo clippy` / `cargo test` にすべて `--locked` を付けているが、`rust.yml` と `spec.yml` には `--locked` が付いていない。
- **それで見逃される具体的な失敗の例**:
  - `flake.nix` の `packages.default`（`cargoBuildFlags` や `wrapProgram`、macOS の `gpui/runtime_shaders`）が壊れても全 CI が green になり、`docs/usage.md:66` の `nix build` が失敗する状態でマージされる。
  - PR で `spec.yml` から `PROPTEST_CASES: 10000` を丸ごと削除したり、`.cargo/mutants.toml` に 1 行形式で `exclude_globs = ["crates/pathos-core/src/file.rs"]` を追加しても、`added-lines.yml` は pass する。
- **直し方の案**:
  - CI に `nix build --no-link` ジョブを追加し、`rust.yml` / `spec.yml` の全 `cargo` コマンドに `--locked` を付与する。
  - `kani.yml` の cover property チェックを共通スクリプト化し `scripts/verify kani` からも呼ぶ（かつ `covered` が空のときも失敗させる）。
  - `check-escape-hatches.sh` で 1 行配列の解析、閾値行の削除検出、ブロックコメントおよび空理由 `#[ignore = ""]` の検出を補う。

---

### 7. [medium] `pathos-agent` の結合経路 (`pathos-mcp` バイナリ・ソケット上限・セキュリティ境界・`difference` / `mcp` 分岐) にテスト漏れがある（問い 1 / crate: `pathos-agent`, ADR 5・6・35・36）

- **重要度**: `medium`
- **場所**:
  - `crates/pathos-agent/src/bin/pathos-mcp.rs:1-24`
  - `crates/pathos-agent/src/socket.rs:86-126`, `L151-165`, `L181-195`
  - `crates/pathos-agent/src/mcp.rs:15-27`, `L269-286`
  - `crates/pathos-agent/src/difference.rs:10-107`
  - `.github/workflows/mutants.yml:37-40`
- **何が守られていないか**:
  - 【事実】`mcp.rs` のテストは `call` クロージャをモックし、`socket.rs` のテストは `Proposals` を直接呼んでおり、**`pathos-mcp` バイナリ → Unix socket → `Listener` → `Proposals` → `Request::File` → `pathos-measure`** の結合テストがない。
  - 【事実】`socket.rs` では `MAX_REQUEST` (1 MiB) 超過時の拒否 (`L151-154`)、1 接続上の複数リクエスト、切断時処理、シンボリックリンク親ディレクトリの拒否 (`L92`) が未テスト。また `Listener::bind` (`L91-94`) はディレクトリのパーミッション (`mode & 0o077 == 0`) のみ確認し、**所有者 UID の一致**は検査していない（ADR 35 の "Nothing outside the user's account may read or change the project" に対する未検証境界）。さらに `bind` 後に `set_permissions(0o600)` しており (`L105-106`)、`Listener::drop` (`L134-138`) でも `incoming()` スレッドは終了しない。
  - 【事実】`mcp.rs:15-27` の `COMMANDS` 説明文字列は手書きで、`pathos-core::Command` に新規コマンドが増えても追従を保証するテストがない。また `get_measurements` での `TooLong` エラーや想定外 `Response` バリアント返却 (`L272-278`) が未テスト。
  - 【事実】`difference.rs` では `unmute` / `solo` / `unsolo` / `remove-tempo` / 右パン (`R..`) / 複数ノート新規クリップ (`with N notes`) が未テスト。
  - 【事実】`mutants.yml:37-40` の週次実行は `pathos-core` のみで、`pathos-agent` (145 mutants) と `src/editor.rs` (130 mutants) は PR の変更行しかミューテーションされない。
- **それで見逃される具体的な失敗の例**:
  - `pathos-mcp` のコマンドライン起動や `PATHOS_SOCKET` 読み取りの配線が壊れても全ユニットテストが通る。
  - 巨大プロジェクトで `Request::File` のレスポンスや `Propose` が `MAX_REQUEST` を超えた場合の挙動や、ソケット切断時のハングに気づけない。
  - 新コマンド追加時に MCP のツール説明 (`COMMANDS`) だけ古いまま取り残され、エージェントが新コマンドを使えない。
- **直し方の案**:
  - `pathos-agent/tests/` に実ソケット経由で `mcp::serve` と `Listener` / `Proposals` を繋ぐ結合テスト（`get_measurements` の `TooLong`・`MAX_REQUEST` 超過・複数リクエスト・切断含む）を追加する。
  - `socket.rs` で親ディレクトリの所有者 UID チェックとテストを追加し、`mcp.rs` で全 `Command` 語彙が `COMMANDS` に含まれることを検証するテストを追加する。
  - `mutants.yml` の週次実行に `agent` と `editor` も加える。

---

### 8. [medium] `pathos-playback` の `output.rs`・`Transport` の並行性・`handoff` の終了/連続 publish パターンが検証から漏れている（問い 1・2 / crate: `pathos-playback`, ADR 17・18・22・25）

- **重要度**: `medium`
- **場所**:
  - `crates/pathos-playback/src/output.rs:42-121`
  - `crates/pathos-playback/src/engine.rs:21-86`, `L113-164`
  - `crates/pathos-playback/tests/handoff.rs:58-101`
  - `crates/pathos-playback/tests/engine.rs:8`, `crates/pathos-playback/tests/no_allocation.rs:4`
  - `tla/SnapshotHandoff.tla:1-137`
- **何が守られていないか**:
  - 【事実】`output.rs` はテストが 0 件。`build<T>` (`output.rs:103-116`) のチャンク分割や `FromSample<f32>` 変換、エラーコールバックの `eprintln!` (`L117`)、および `channels == 0` / `channels > BUFFER` でのパニック（ゼロ除算・`chunks_mut(0)`）が未検証。
  - 【事実】`Engine::fill` (`engine.rs:123-124`) も `channels == 0` でゼロ除算パニックし、`no_allocation.rs` は 2ch のみ検証している（1ch / 4ch や `output.rs` のコールバック変換ループは未検証）。
  - 【事実】`loom` で実行されるテストは `every_interleaving_keeps_the_invariants` (`tests/handoff.rs:97-101`) の **1 件のみ**（実測確認済）で、`publish` の直後に必ず `collect` を呼ぶ 1 パターンしか試していない。「`collect` なしで連続 `publish`（スロット上書きと `retired` 満杯時の据え置き）」や「`Publisher` / `Player` のドロップ順序」の並行インタリーブは loom で検証されていない。また `tla/SnapshotHandoff.tla` にも終了時の解放 (`Drop`) がモデル化されておらず、TLA+ と Rust の対応は人手依存。
  - 【事実】`Transport` (`engine.rs:21-86`) は全フィールドに `Ordering::Relaxed` を使っており、`tests/engine.rs` が `#![cfg(not(loom))]` のため並行テストが一切ない。
- **それで見逃される具体的な失敗の例**:
  - UI が `seek(0)` して直後に `play()` を呼んだとき、弱メモリ順序環境 (ARM64) で `Relaxed` により audio thread が `playing = true` を先に観測し、旧位置の音声を 1 ブロック鳴らす可能性がある。
  - オーディオドライバが 0ch や想定外チャンネル数を返したとき、オーディオコールバック内でパニックする。
  - `handoff.rs` で `retired` 満杯のまま連続 `publish` → 終了する経路でメモリリークや二重解放が入っても、今の loom テストだけでは捕捉しきれない恐れがある。
- **直し方の案**:
  - `tests/handoff.rs` の loom テストに「連続 `publish`（途中 `collect` なし）」「audio thread 内で `snapshot.id` を読む瞬間に `freed` に含まれないことの直接アサート (`NoUseAfterFree`)」を追加し、`Transport` の `seek`+`play`・`meter`+`take_peaks` も loom テスト対象にする。
  - `output.rs` のサンプル変換・チャンク処理を `cpal::Device` 非依存の純関数に抽出してテストし、`Engine::fill` で `channels == 0` を安全にガードする。

---

### 9. [medium] `pathos-core` の Kani ハーネスと Lean 定理に未カバーの関数・性質がある（問い 1 / crate: `pathos-core`, `spec/`, ADR 3・11・15・16・19）

- **重要度**: `medium`
- **場所**:
  - `crates/pathos-core/src/project/verification.rs:113-134`, `L260-265`, `L496-516`
  - `crates/pathos-core/src/schedule.rs:56-63`, `L101-129`
  - `crates/pathos-core/tests/differential.rs:537-552`
  - `spec/Pathos/Schedule.lean:55-112`, `spec/Pathos/History.lean:34-101`
- **何が守られていないか**:
  - 【事実】Kani には ADR 16 で除外されたノート付き `SplitClip` 以外にも、**`TempoMap::sample_of` (`schedule.rs:56-63`)・`Project::schedule`・`History::edit`・`Document::load` / `Edit::from_str`** のハーネスがない（`tick_to_micros_does_not_overflow` は `tick_to_micros` のみ）。
  - 【事実】`delete_clip_with_1_note` (`verification.rs:260-265`) と `add_note_*` / `remove_note_*` はすべて **`clips = 1` 固定**で、2 クリップ間でノート配列の境界 (`note_range`) を壊さないことは Kani で未検証。
  - 【事実】Lean モデル (`spec/theorems.txt`) には「聞かれるトラックの各ノート（`on != off`）について、スケジュール上で `On` と `Off` が 1 対 1 に対応し必ず `On` が `Off` より前に来る」定理がない。しかし下流の `crates/pathos-render/src/lib.rs:151-153` は `sounding.remove(&(track, pitch)).expect("a note-off follows the note-on of its note")` とパニック前提でこれに依存している。また `History.edit` 全体の well-formedness 保存も定理化されていない。
  - 【事実】`Document::load` (`crates/pathos-core/src/file.rs`) と Lean の `Request.parse` について、異常・境界入力（空白・タブ・不正行・`micros`/`schedule`・オーバーフロー数値）の差分テストがない（正常保存のラウンドトリップのみ）。
- **それで見逃される具体的な失敗の例**:
  - `TempoMap::sample_of` の `u128` 計算式をリファクタした際に特定の大ティック×高レートでオーバーフローしても、Kani では検出されない。
  - `DeleteClip` の `note_range` に境界バグがあり隣接クリップのノートまで巻き込んで削除しても、`clips = 1` の Kani ハーネスでは検出されない。
  - スケジュール並び替え規則を変更した際、`On` より前に `Off` が来て `pathos-render` の `Score::new` が実行時パニックする仕様不整合を Lean 定理が防げない。
- **直し方の案**:
  - `TempoMap::sample_of` 用の小型 Kani ハーネス（2 テンポ変化＋任意 `tick <= MAX_TICK + PPQ`・任意 `rate`）を追加する。
  - Lean の `Schedule.lean` に「well-formed なプロジェクトの `schedule` では同一 `(track, pitch)` の `On`/`Off` が交互に現れ、すべての `Off` に先行する `On` がある」定理を追加する（最初は `sorry` でも ADR 4 の方針通り）。
  - `file.rs` のパーサと Lean `Request.parse` を不正入力込みで突き合わせるテストを追加する。

---

### 10. [medium] `kani.yml` の 30 runner 無キャッシュ並列起動など、CI の費用対効果が低い重複・非効率がある（問い 4 / CI ワークフロー・フック全般）

- **重要度**: `medium`
- **場所**:
  - `.github/workflows/kani.yml:17-56`
  - `.github/workflows/rust.yml:26-51`
  - `.github/workflows/spec.yml:90-98`
  - `.githooks/pre-commit:7-8`
  - `.github/workflows/zizmor.yml:22-36`
  - `crates/pathos-core/tests/differential.rs:241-266`
- **何が守られていないか（費用対効果・重複の問題）**:
  - 【事実】`kani.yml` は `list` + 29 ハーネス = **計 30 ジョブ**を毎 push・毎 PR で起動し、**キャッシュなしで毎回 `cargo install --locked kani-verifier` + `cargo kani setup`** を 30 回実行している。さらに `paths:` フィルタが無いため、`README.md` や `src/theme.rs` の変更でも 30 ジョブすべてが走る。また軽いハーネス（`add_track`, `set_master_volume`, `set_track_*_with_1_mixed` など）も重いノート系ハーネスと同様に 1 runner を丸ごと占有している。
  - 【事実】`rust.yml:26-51` は 1 ジョブ内で `nix develop -c` を **9 回**起動し、`cargo test -p` を 5 回別々に呼んでいる（`scripts/verify:159` は 1 回の `cargo test --workspace --exclude pathos-core` で実行）。
  - 【事実】`spec.yml:91` で `cargo test -p pathos-core`（`generators_reach_every_rule` を含む）を実行した直後に、`spec.yml:97` でもう一度同じ `generators_reach_every_rule` を再実行している（実測 6.5 秒の重複）。また `differential.rs:243` は 1 トレースごとに `pathos-model` プロセスを新規起動しており、CI (`PROPTEST_CASES: 10000`, 2 テスト) では **20,000 回のプロセス起動**が発生する。
  - 【事実】`.githooks/pre-commit:7-8` は `check-escape-hatches.sh --staged` を実行した直後に `scripts/verify quick` 経由で再度 `check-escape-hatches.sh origin/main` を走らせる。重複だけでなく、**未ステージの作業中ファイルに `// TODO` があるとステージ済みの綺麗な変更までコミット拒否される**副作用がある。
  - 【事実】`zizmor.yml:24, 35` は SARIF 出力とエラー終了のためオンライン監査を 2 回走らせている。
- **それで見逃される具体的な失敗の例**:
  - CI 待ち時間と runner 消費が膨らみ（特に Kani 30 ジョブ）、開発ループの遅延やワークフロー変更時のコスト増を招く。
  - `pre-commit` で未ステージのメモ書きがコミットをブロックし、`--no-verify` を誘発する恐れがある。
- **直し方の案**:
  - `kani.yml` で `~/.kani` と `kani-verifier` バイナリをキャッシュし、重いハーネス（ノート系・`split_clip` 等）だけ独立ジョブにして軽いハーネス群は 1〜2 ジョブに束ねる。さらに `crates/pathos-core/`・`Cargo.*`・`.github/workflows/kani.yml` 変更時のみ実行する（ブランチ保護の required check は `verify` ジョブを常時成功させるパスフィルタパターンで維持）。
  - `rust.yml` は 1 回の `nix develop -c bash -c '...'` 内でまとめ、`cargo test --locked --workspace --exclude pathos-core` に統合する。
  - `spec.yml` は `generators_reach_every_rule` を 1 回だけ `--nocapture` で実行して出力を保存するか、`--skip generators_reach_every_rule` で重複実行を無くす。また `pathos-model` にリセットコマンド（例: `reset`）を設けてプロセスを再利用すれば、同じ時間で差分テスト件数を増やせる。
  - `pre-commit` 時は `scripts/verify quick` に「ステージ済み検査済み」フラグを渡して `check-escape-hatches.sh` の重複と未ステージ誤爆を防ぐ。

---

### 11. [medium] `pathos-render` のプロパティテストが固定条件（120 BPM・単一クリップ）に偏り、エンベロープや WAV 4 GiB 境界も未検証（問い 1・2 / crate: `pathos-render`, ADR 15・20・21・24）

- **重要度**: `medium`
- **場所**:
  - `crates/pathos-render/tests/render.rs:32-54`, `L116`, `L204-266`
  - `crates/pathos-render/src/wav.rs:28-30`, `crates/pathos-render/tests/wav.rs:70-78`
- **何が守られていないか**:
  - 【事実】`render.rs` のヘルパー `project()` (`L32-54`) は常に「120 BPM・テンポ変化なし・トラック 2 本・tick 0 から 3 秒固定の 1 クリップ」しか作らない。そのため、テンポ変化あり・複数クリップあり・tick > 0 開始クリップ・ループ/メトロノーム併用でのプロパティテストがない。
  - 【事実】音量テストは `500` と `0` のみ（`volume > 1000` のゲイン増幅や `master_volume` × `track.volume` × `pan` の組合せ未検証）、パンテストは `0, 150, 200` のみ（左側中間値なし）。また 5ms リニアアタック/リリースエンベロープ（特に 10ms 未満の短ノート）の形状を直接検証するテストがなく、固定ハッシュとローカル REAPER 比較に間接依存している。
  - 【事実】`wav::write` の 4 GiB 超過エラー (`wav.rs:28-30`) はテストされていない（`sample_rate` 過大のみテスト）。また `pathos-render` (154 mutants) は `mutants.yml` 対象外。
- **それで見逃される具体的な失敗の例**:
  - 5ms 未満の極短ノートのエンベロープ計算や、`pan < 100` の左寄りパン計算式にバグが入っても、固定ハッシュ更新時に理由を誤認するとそのまま見逃される。
  - テンポ変化中にブロック分割レンダリング (`Score::render`) とメトロノーム/ノートが噛み合わないバグをプロパティテストが拾えない。
- **直し方の案**:
  - `render.rs` のプロパティテスト生成器に任意のテンポ変化・複数クリップ・任意の `volume (0..=2000)`・`pan (0..=200)`・`master_volume` を含め、エンベロープの単調増加/減少（短ノート含む）を直接検証するテストを追加する。`pathos-render` も `mutants.yml` に加える。

---

### 12. [medium] ドキュメント・ADR・設定ファイル間の食い違いが複数存在する（問い 5 / ドキュメント・設定全般）

- **重要度**: `medium`
- **場所**:
  - `.cargo/mutants.toml:13-21` vs `.github/workflows/mutants.yml:47-50` & `scripts/verify:133-136`
  - `docs/adr/0034-run-the-comparison-with-reaper-locally-not-in-ci.md:54-55`, `L75` vs `scripts/check-reaper-line.sh:19-20` & `AGENTS.md:36-41`
  - `docs/verification.md:11-16`, `L45`, `L91` vs `scripts/verify:124`, `L171` & `.github/workflows/spec.yml:93`
  - `README.md:48-51` vs `crates/pathos-agent/src/socket.rs:6-7` & `Cargo.toml:13`
  - `docs/adr/0013-measure-the-tests-by-mutating-the-rust-core.md:39-41`, `docs/adr/0017-specify-the-concurrent-parts-in-tla-and-check-them-with-tlc.md:125` vs `tla/SnapshotHandoff.cfg:3-4`
- **何が守られていないか**:
  - 【事実】`.cargo/mutants.toml:17, 20` に `Score::add_clicks` (`pathos-render`) と `lufs` (`pathos-measure`) の除外ルールがあるが、`mutants.yml` も `scripts/verify mutants` もこれら 2 crate を対象にしておらず、実態と設定が食い違っている。
  - 【事実】ADR 34:54-55 は `REAPER comparison: passed`（キー無し）と記載し、L75 は `AGENTS.md` にコマンドと実行条件が書かれていると記載しているが、実際にはキー付き `REAPER comparison: passed <key>` が必須で、`AGENTS.md` には `nix develop .#reaper ...` のコマンドも対象 crate 名も書かれていない。
  - 【事実】`docs/verification.md:12` は `scripts/verify mutants` を "as CI does for a pull request" と説明するが、`scripts/verify:124` は 2 点ドット `git diff "$base"` を使うため、ローカルで `origin/main` が先行していると他ブランチの変更まで逆差分として拾う（`check-reaper-line.sh:14` は 3 点ドット `$base...$head` を使用）。
  - 【事実】`docs/verification.md:91` の目視確認チェックリスト "The window is looked at" は `src/main.rs`, `src/agent.rs` のみトリガし、見た目や配置を直接変える `src/theme.rs` と `src/layout.rs` の変更では発火しない。同様に L45 は `spec/Audit.lean` / `spec/Main.lean` で発火しない。
  - 【事実】`README.md:48-51` は Windows でのビルド・実行手順を案内しているが、`Cargo.toml:13` で `pathos` が `pathos-agent` に依存し、`crates/pathos-agent/src/socket.rs:6-7` が `std::os::unix::*` を無条件インポートしており、CI にも Windows ビルドがない（【推測】現状 Windows ではビルドエラーになるはず）。
  - 【事実】`docs/verification.md:11` は `scripts/verify` が `PROPTEST_CASES` 2,000 で走り CI (`spec.yml:93`) が 10,000 で走る差を明記していない。また ADR 13 は `pathos-core` のみ記載（実際は PR で `agent`/`editor` 追加）、ADR 17:125 は `Snapshots=15, Capacity=3` 記載（実際は `Snapshots=30, Capacity=1`）、ADR 11 は `amendedby: 16` のリンクが未記載。
- **それで見逃される具体的な失敗の例**:
  - `.cargo/mutants.toml` を読んだ開発者・エージェントが `pathos-render` や `pathos-measure` もミューテーションテストで守られていると誤認する。
  - `src/layout.rs` や `src/theme.rs` を変更した際、`verification-checklist.sh` が目視確認を出さず UI 崩れに気づきにくくなる。
  - Windows ユーザーが `README.md` 通りにビルドして即座に失敗する。
- **直し方の案**:
  - `pathos-render` / `pathos-measure` を `mutants.yml` と `scripts/verify` に追加する（あるいは対象外のままなら `mutants.toml` の理由を明記）。
  - `scripts/verify:124` を 3 点ドット差分 (`git diff "$base...HEAD"` + 作業ツリー差分) に修正し、`docs/verification.md` のチェックリスト対象パスに `src/theme.rs`, `src/layout.rs`, `spec/Audit.lean`, `spec/Main.lean` を加える。
  - `README.md` の Windows 記述と `pathos-agent` の Unix 依存（ADR 35）を整合させ、ADR 34 / 13 / 17 の記述差分を整理する。

---

### 13. [low] `src/editor.rs`・`src/file.rs`・`src/agent.rs` のエッジケースと `spec.yml` の `sorry` 集計の粗さ（問い 1・2 / crate: `pathos`, CI）

- **重要度**: `low`
- **場所**:
  - `src/editor.rs:280-288`, `L395-405`
  - `src/file.rs:97-103`
  - `src/agent.rs:23-34`
  - `.github/workflows/spec.yml:29`
  - `crates/pathos-core/tests/differential.rs:81-85`
- **何が守られていないか**:
  - 【事実】`Editor::handle` (`src/editor.rs:280-288`) は `self.document.edit(edit)` の成否を待たずに `self.selected = Some(self.history().next_clip_id())` を先に代入する。`AddClip` が拒否された直後は `selected()` がフィルタして `None` を返すが、直後に別の経路（例: エージェント提案や `SplitClip`）でその ID のクリップが作られると意図せず選択状態になる。このケースのテストはない。
  - 【事実】`Saved::directory` (`src/file.rs:97-103`) は `Path::new("song.pathos")` のような相対ファイル名に対して `parent()` が `Some("")` を返すため、`current_dir()` にフォールバックせず空パス `""` を返す（テストは常に絶対パスのみ）。
  - 【事実】`spec.yml:29` は `grep -rw sorry spec --include='*.lean' | wc -l` で未完了証明を数えているが、コメント中の `sorry` という単語も誤カウントする（正確な数は直後の `Audit.lean` が出力済み）。
  - 【事実】`differential.rs:81-85` の `every_error_has_a_message` は空文字列でないことしか見ておらず、エラー文言の重複や誤対応は検出できない。
- **それで見逃される具体的な失敗の例**:
  - `cargo run -- song.pathos` で起動し「名前を付けて保存」を開いたとき、ダイアログ初期ディレクトリが空パスになる。
  - Lean のコメントに `-- proved without sorry` と書くと、`spec.yml` のサマリーに `Unfinished proofs (sorry): 1` と誤表示される。
- **直し方の案**:
  - `Editor::handle` では `edit` 成功後（または実行前に ID を控えて成功後）に `selected` と `scroll` を更新し、拒否後＋提案適用のテストを追加する。
  - `Saved::directory` で `parent().filter(|p| !p.as_os_str().is_empty())` とし、相対ファイル名のテストを追加する。
  - `spec.yml:29` の `grep` ステップは削除し、`Audit.lean` の出力 (`N theorems, M unfinished`) に一本化する。

---

## 問い 1〜5 ごとの総括と「問題なし」の観点

### 問い 1: crate 別・ADR 別のカバー状況
- **`pathos-core` (ADR 2, 3, 4, 9, 10, 11, 12, 13, 14, 16, 19, 20, 21, 24, 32)**: 全体の核は Lean 48 定理すべて `[proved]`・差分テスト・週次/PR ミューテーション・Kani 29 ハーネスと非常に堅牢。一方、手書き期待値テストの薄さ（指摘 5）、`sample_of` / `schedule` / 複数クリップ間ノート操作の Kani 不足（指摘 9）、`generators_reach_every_rule` の合算バグ（指摘 4）が穴。
- **`pathos-render` (ADR 15, 20, 21, 22, 23, 24, 32)**: 3 OS (`x86_64-linux`, `aarch64-linux`, `macos`) での決定性ハッシュ検証は **問題なし**（`rust.yml:56-72`, `render.rs:306, 599`）。一方、`mutants` 対象外・プロパティテストの固定条件・エンベロープ直接検証不足が穴（指摘 11）。
- **`pathos-measure` (ADR 6, 36)**: **最大のギャップ**。388 mutants が `mutants` 対象外、BS.1770 相対ゲート未テスト、`true_peaks` 枝刈り未検証、`peak` 許容幅 12.1 dB、テンポ変化/ソロ/`master_volume` 未テスト（指摘 1）。また ADR 6 の「既知問題の評価セット」は未整備（ADR 36:68 も未了と明記）。
- **`pathos-agent` (ADR 5, 6, 35, 36)**: 基本的なユニットテストと実ソケットのラウンドトリップはあるが、`pathos-mcp` 結合・`MAX_REQUEST` 超過・UID 検証・`Proposals` の別ドキュメント切替 stale 判定・`apply` 途中失敗・週次 `mutants` が漏れ（指摘 2・7）。
- **`pathos-playback` (ADR 17, 18, 22, 23, 25)**: `Engine` 再生一致と `no_allocation.rs` の 2ch 検証、TLC (`SnapshotHandoff.tla`) の実行自体は良好。一方、`output.rs` テストゼロ、`Transport` の並行テストなし、loom が 1 パターン限定（指摘 8）。
- **`pathos-reaper` (ADR 7, 33, 34)**: 変換器単体テストは CI で動作するが、REAPER 比較はローカル限定・自己申告、生成ケース 8 件かつ削除/Undo 系なし（指摘 5）。他 DAW 保存リファレンス（ADR 7）は未着手。
- **`pathos` (`src/`, ADR 26〜31, 35, 36)**: `layout.rs` / `editor.rs` / `meter.rs` / `readout.rs` / `file.rs` の単体テストは充実している一方、`main.rs` (1,462 行) と `agent.rs::proposal_card` は無テスト、`layout.rs::tick_at` は ADR 32 と不整合（指摘 2・3・13）。

### 問い 2〜5 のうち「問題なし」と判断できる観点と根拠
- **依存方向と境界の強制**: **問題なし**。`tests/architecture.rs:51-154` がワークスペース crate 間の DAG・`pathos-core` 無依存・`src/` のレイヤー方向と `gpui` 使用制限を AST 解析で自動検証し、自己テストも含んでいる。`deny.toml:38-41` も `gpui` / `cpal` のラッパー制限を強制している。
- **禁止 API と安全リント**: **問題なし**。`Cargo.toml:46-54` (`unwrap_used`, `undocumented_unsafe_blocks`, `allow_attributes_without_reason` = `deny`)、`crates/pathos-core/src/lib.rs:7-10` (`as_conversions` = `deny`)、`clippy.toml:7-15` (時計・sleep・環境変数の直接参照禁止) が CI の `cargo clippy --workspace --all-targets -- -D warnings` で全 crate に適用されている（全 `crates/*/Cargo.toml` が `[lints] workspace = true` を継承していることを確認済み）。
- **Lean 証明の健全性・後退防止**: **問題なし**。`spec/Audit.lean:45-64, 123-126` がカスタム公理の宣言・依存を禁止し、実測で 48/48 定理が `[proved]` (`sorry` 0 件) かつ `theorems.txt` / `definitions.txt` と完全一致することを確認した。`spec.yml:61-70` が定理削除と `[proved]` → `[unfinished]` の後退を CI で確実に落とす。
- **サプライチェーン・ワークフロー監査 (ADR 8)**: **問題なし**。`rust.yml:30` + `advisories.yml` の日次 `cargo deny`、`zizmor.yml` の全ワークフロー検査、全 Action の SHA ピン留め・最小 `permissions`・`persist-credentials: false`、`.github/dependabot.yml` の 7 日 cooldown が一貫して設定されている。
- **TLA+ TLC の実行結果チェック**: **問題なし**。`tla.yml:30` と `scripts/verify:146` の両方で、TLC の終了コード 0 の罠を避けるため `Model checking completed. No error has been found.` の出力を明示的に `grep` 検証している（実測 3.57s で pass）。

---

## 確認できなかったこと

以下の項目は制約や環境上、実行・直接観察できていない。

1. **REAPER 比較テストの実実行** (`nix develop .#reaper -c cargo test -p pathos-reaper -- --ignored`): 未実行。実際の REAPER 7.86 とのサンプル誤差や JSFX の実行時挙動、8 ケース実行の所要時間は未確認。
2. **Kani ハーネスの実実行** (`scripts/verify kani` / `cargo kani`): 未実行。各 29 ハーネスの実際の検証時間・ピークメモリ・cover property の充足状態は未確認。
3. **`cargo mutants` の全体実行** (`pathos-core` ほか全 crate): 未実行（`pathos-measure` の `integrated_loudness` 23 mutants のみ実測）。`pathos-core` / `pathos-agent` / `src/editor.rs` / `pathos-render` / `pathos-playback` / `src/layout.rs` に現時点で何件の missed mutant があるかは未確認。
4. **GitHub リポジトリ側の設定**: `main` のブランチ保護・ruleset でどのジョブ（`verify`, `stated`, `conformance`, `lint`, `render`, `tlc`, `added-lines`, `mutants` 等）が required になっているか、また CodeQL default setup・secret scanning・push protection・Dependabot alerts が実際に有効かはローカルからは確認不可（ADR 8:83-84 にもリポジトリ外設定と明記）。
5. **macOS (`aarch64-darwin`)・ARM64 Linux・Windows でのビルドおよびテスト**: 現在の環境は Linux のみのため、macOS の `gpui/runtime_shaders`・`bindgenHook` や、Windows で `pathos-agent/src/socket.rs` の `std::os::unix` により実際にコンパイルエラーになるかは未実測（コード読解からの推測）。
6. **実オーディオデバイス (`cpal`) と GUI ウィンドウ描画の実行時挙動**: `Output::open()` の ALSA/PipeWire 接続、`JetBrains Mono` フォントの読み込みと実際のレンダリング結果・画面崩れは未確認。
