# 宿主层深挖（architecture-deepening）规格

## 背景

工单 10–21 之后，库层模块深度良好，但宿主 `src/bin/moe-frame.rs` 没有 module 边界：工单 18 起的新职责全部绕开行为核心、以「字段 + 方法」长在 FrameApp 上，宿主侧行为零自动化覆盖，不变量靠多处手抄维系。2026-11-21 架构评审（improve-codebase-architecture）产出六个 deepening 候选，逐个 grilling 定案，决策全文见 `.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md`（本专题的父决策记录）。

## 共同形状

- 新 module 一律放库层、egui-free，事件进 / 决策出；宿主只负责采样、渲染、执行。
- module 之间不横向通信，一切经宿主转交。
- 测试只留一个测试面（replace-don't-layer）：被吸收的旧公开面（settings_draft、window_geom 纯函数、core 的落位枚举等）的用例改打新 interface，不双测。
- 每票沿用既有流程：TDD 在新 interface 上先行，全量测试收尾，双轴 code-review，实施笔记。

## 总顺序

票 01（display_data）→ 02（settings_session）→ 03（Playlist 导航收拢）→ 04（config_sync）→ 05（几何状态机）→ 实机清单（仓库根目录 real-machine-check-10-21.md，验新代码）→ v1-exe 工单 22/23 发布门 → 票 07（双解码合流）。票 06（等价测试）无阻塞，是票 07 与发布门的共同前置，须在发布门之前完成。票 05 是唯一可无损推迟到 V1 之后的一项。

## 不做的事

- 不改任何用户可见行为；实机清单的既有口径（含多显示器已知限制）原样保留。
- 不动 ADR-0001/0002/0003 的任何语义；三 P1 配置键不升格。
- 候选 5 合流前不碰工单 09 冻结接缝的实现（票 06 只加测试，不改实现）。
