## Agent skills

### Issue tracker

Issues and specs live as markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical roles, each label string equal to its name: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` and `docs/adr/`. See `docs/agents/domain.md`.
### 代码地图

宿主入口 `src/bin/moe-frame.rs`（main 与装配），宿主行为按职责在 `src/bin/moe-frame/` 分模块；可复用逻辑在 `src/` 的库里，`tests/` 直接引用。改动前先看 `README.md` 的目录段。

### 检查

唯一入口是 `scripts/check.sh`（`cargo fmt --check` + `clippy -D warnings` + `test --all-targets`），CI 跑同一套。不要另建第二套检查命令，直接扩这个脚本。
