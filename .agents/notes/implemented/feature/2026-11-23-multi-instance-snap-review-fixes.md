# 多实例吸附专题双轴审查修复（2026-11-24）

对集成分支 feature/multi-instance-snap @ ff150b3 相对 origin/main 的双轴 code review（Standards + Spec）发现的逐条处置。修复分支 impl/multi-instance-snap-review-fixes，提交：代码两笔（snap 拆分/求解器/锚边；config/级联/启动装配）+ 文档一笔。

## Spec 轴

- **S1 Shift 拆组跨进程边界**：注记收口。spec 第 14 节新增注记，写明 solo 无跨进程通路、伙伴仍可能跟随、工单 03 该验收项保持未勾的原因；正文不动。不视为达成。
- **S2 load 写回留尾**：真修。load 的修复/迁移写回改走 ConfigStore::update 唯一通道（空改动调用，重读磁盘当前文档后重跑幂等修复/迁移再写回），与 setter 同纪律；残余的重读→写回无锁窗口与 setter 路径同级，是既定设计口径。新增测试 load_repair_write_back_preserves_other_instances_current_data。
- **S3（蔓延）抑制连带解除锚边**：真修解耦。锚边派生改按原始接触（raw_contact_ids），跟随/成组派生保持减抑制边；新增测试 detached_partner_still_anchors_resize_but_never_pulls。
- **S4（不符）级联基点**：真修。宿主启动瞬间枚举活伙伴（snap_enum::startup_partner_positions，pid→编号锁映射、含隐藏窗），纯规则 instance::cascade_base_position（编号最大者）定基点，ConfigSync::startup_for_instance_with_cascade 注入；落盘段口径降为兜底。新增规则单测与 config_sync 实时基点测试。
- **S5（不符）跟随节奏**：注记收口。spec 第 14 节新增注记：16ms 只指移动期间，静止 1000ms 心跳换零打扰，首动检测延迟可达一个心跳周期。
- **S6（不符）松手吸附优先级**：真修。AxisSolver 跨组裁决改类别先行（贴合压过对齐），同类再比间隙/生成序；新增回归测试 solve_attach_on_one_edge_beats_smaller_gap_alignment_on_the_other（无工作区候选干扰的构造）。

## Standards 轴

- **H1 工单 03 check 项措辞自相矛盾**：已改述为与 01/02/04 相同的「本体未运行 + 替代口径」口径。
- **H2 README 目录段缺 snap_enum**：已补 snap_enum、instance_launch（J3 新增）与库 snap_follow.rs。
- **H3 Avoid 词「延时」漏检**：代码/测试注释、工单 03 与其实现笔记中的「延时」全量改「间隔」（英文标识符 next_frame_delay 不动；GLOSSARY 词条本身的 Avoid 列表保留原词）。
- **J1 Primitive Obsession**：AxisCandidate.kind 由 u8 + KIND_* 常量改真枚举 SnapCandidateKind（声明序即优先级序）。
- **J2 Divergent Change**：src/snap.rs（850 行、02+03 混合）拆分——跟随引擎/抑制/轮询调度/锚边移入新库模块 src/snap_follow.rs，snap.rs 只留工单 02 的判定/派生/求解，两文件头各自如实描述。src/config.rs 的膨胀属同一配置存储的内聚增长，不拆。
- **J3 装配外溢**：moe-frame.rs run() 内的 --instance 解析、认领分支、自启恢复裁决组装抽到新宿主模块 src/bin/moe-frame/instance_launch.rs（claim_from_args / live_cascade_base / recover_marked_instances），run() 只分发。
- **J4 Data Clumps**：不修（判断性发现）。rect+monitor+work_area 在 CandidateWindow/PartnerFrame/SnapPartner/FollowPartner 间成组出现，但四者语义不同（枚举事实/伙伴事实/求解输入/带身份的跟随输入），强行合并成一个共用类型会造出谁都不完全贴合的假抽象；现状每个类型都是其消费方的最小事实集，接受。

## 验证

- scripts/check.sh 本体未运行（Windows 专用，本 VM 跑不了，不冒充）。
- Linux 测试副本（~/workspace/tmp/refresh-moeframe-test-snapfix.sh）：cargo test --all-targets = 430 passed / 0 failed（基线 424，净增 6：S6 回归、S3 解耦、S2 通道、级联选择规则、实时级联基点等新测试；随模块拆分迁移的既有测试不减）。
- Windows 目标：cargo check --all-targets、cargo clippy --all-targets -- -D warnings、cargo fmt --all -- --check 三项通过。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复读无劈裂/孤儿文档注释、无重复属性；GLOSSARY Avoid 词（含本轮补检的「延时」）对代码与测试 diff grep 零命中（GLOSSARY 词条自身的 Avoid 列表除外）。
