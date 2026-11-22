# Agent Note: 峰值预算收紧到 ≤192MB

Status: implemented

Supersedes（数值部分）: [2026-11-20 产品 Limits 与峰值预算确认](2026-11-20-product-limits-peak-budget.md)

## Problem

2026-11-20 确认的峰值预算是 ≤200 MB，依据是 `decode-validation` 独立验证程序在默认字体路线图片组上的生命周期峰值（142524416–143040512 B）。那套数据来自验证器自己的测量路径：进程、渲染后端与产品宿主不同，不能直接当成完整程序的峰值依据。

完整程序（票 22）用 `scripts/measure-release.ps1` 复测后，13 张样例（含 100MP）轮播 3 分钟的峰值是 **130.82 MB**，稳定工作集 76.68 MB。数字既有实测支撑，就需要一个有推导依据的门槛，而不是沿用验证器的 200 MB。

## Decision

用户于 2026-11-22 确认：**峰值工作集 ≤192 MB**（选项 B）。

推导口径：稳态预算 80 MB + 单次解码缓冲上限 `PRODUCT_LIMITS.max_alloc = 96 MiB`（`src/loading.rs:14`）= 176.7 MB，再留 8% 给分配器与纹理抖动。实测峰值距门槛 61 MB（47%）余量。产品 Limits 不变（12000×12000、96 MiB）。

测点固定为 `Get-Process` 采 `PeakWorkingSet64`，覆盖至少一轮含 100MP 样本的完整轮播；峰值与稳定工作集分开报告，不拿 Limits 当整个进程的内存上限。

## Alternatives considered

- 选项 A：≤160 MB（实测 +23%）。未采用：不同图集几百 KB 就会擦线，会变成假失败。
- 选项 C：≤256 MB。未采用：等于放弃约束，起不到预算门的作用。
- 选项 D：写成「稳态预算 + 112 MB」的相对公式（现展开为 192 MB）。未采用：与 B 实质等价，但要多维护一条派生规则；改预算时重算一次即可。
- 维持 200 MB。未采用：它是验证器口径的产物，没有推导链；有了完整程序实测就该按实测立门槛。

## Verification

- `docs/release-measurements.md` 留存环境、构建命令、口径、样本与复现命令；峰值 130.82 MB、稳定 76.68 MB、轮播 3 分钟工作集 69.14–87.40 MB。
- `spec.md` 的解码门禁条、NFR-1 与未决项第 3 条已同步为 192 MB；票 22 对应验收条已勾。
- `scripts/check.sh` 全绿。

## Consequences

- `decode-validation` 的 200 MB 表述、票 10 的历史记录都补了「已被 192 MB 取代」的指向，保留原文不重写。
- 若日后放宽 `max_alloc`，peak 门槛必须按同一推导重算并重新确认，不静默改值。