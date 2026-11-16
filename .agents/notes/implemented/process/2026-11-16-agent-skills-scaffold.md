# Agent Note: 工程 skill 的 issue 与领域文档入口

Status: implemented

## Problem

工程 skill 需要知道 issue 放哪、triage 用什么词、领域文档怎么读。这个目录当时没有 git remote，也没有 `AGENTS.md`。不记下入口，后续 skill 会各自猜测。

## Decision

工程 skill 的仓库入口是根目录 `AGENTS.md` 的 `## Agent skills`。Issue 是本地 markdown，约定在 `docs/agents/issue-tracker.md`：一个 feature 一个 `.scratch/<feature-slug>/`，规格是 `spec.md`，票是 `issues/<NN>-<slug>.md`。Triage 五个角色名就是标签字符串，映射在 `docs/agents/triage-labels.md`。领域文档是单上下文：根 `GLOSSARY.md` 与 `docs/adr/`，消费规则在 `docs/agents/domain.md`。这三个文档不存在时，skill 继续工作，不预先创建术语表或 ADR。

## Alternatives considered

- **GitHub Issues** — skill 默认面是 GitHub，远程协作时 `gh` 可直接建票。否掉，因为当时没有 git remote，也没有可访问的仓库。
- **GitLab Issues** — 同样能接远程 issue。否掉，原因相同：没有 GitLab remote。
- **写入 `CLAUDE.md`** — Claude 专用入口也能被部分 skill 读到。否掉，因为用户指定写 `AGENTS.md`，且两个文件都还不存在时不能同时新建。

## Consequences

- **收益**：`to-tickets`、`triage`、`to-spec`、`domain-modeling` 等 skill 有固定入口，不再猜测 tracker。
- **代价与已知上限**：本地文件不能替代远程 issue。一旦这个目录接到 GitHub 或 GitLab 并改用远程 tracker，必须改 `docs/agents/issue-tracker.md` 和本笔记的入口事实，不能只改 `AGENTS.md` 的一句话。
