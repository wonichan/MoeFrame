# Agent Note: 首发走 GitHub Releases 手工上传单文件

Status: implemented

## Problem

V1 的终点是"可验收的 release exe"（见 [V1 终点](2026-11-16-v1-destination-exe.md)），但仓库里只有 `scripts/check.sh` 这一条 CI 检查线，没有任何发布通道：没有 tag、没有 CHANGELOG、没有打包脚本，README 也只说"拷 `target\release\MoeFrame.exe`"。用户要的 V1 首发 `v0.1.0` 没有落点。

发布本身有三个必须先定的取舍：产物从哪来、谁来构建、要不要做成自动化流水线。

## Decision

- **产物就是单个 `MoeFrame.exe`**：本仓库的交付物只有一个静态链接的单文件（见 README 的"免安装"段），不打包 zip、不带配置文件、不带示例图片；配置与日志仍由程序自己在 `%APPDATA%\MoeFrame\` 下生成。
- **人工构建 + 手工上传**：`git tag v0.1.0` 打在 `main` 上，`gh release create v0.1.0` 附上本地 `cargo build --release --bin MoeFrame --locked` 的产物。CI 不参与发布。
- **tag 与 `Cargo.toml` 的 `version` 对齐**：`Cargo.toml` 里本来就是 `0.1.0`，tag 直接沿用，不额外引入版本注入或 `--version` 自报。
- **README 指向 Releases**：首页第一条"免安装"从本地路径改为 Releases 链接，本地构建路径保留给贡献者。
- **发布记录留在 GitHub Releases 正文**，不新建 `CHANGELOG.md`：V1 只有一个版本，逐条维护的文件在这个阶段是纯负担。

## Alternatives considered

- **加一条 `release.yml` 自动构建并上传** — 最强理由：以后每个版本零手工。否决：`check.yml` 用的 `windows-latest` 能编，但首发只有一次，先把通道打通比先搭流水线便宜；自动化要等出现"频繁发版"这个真实需求再做。
- **发 zip（exe + README + 示例配置）** — 最强理由：解压即用，能顺手带说明。否决：程序是单文件静态链接、首次启动会自己拉文件夹对话框，zip 只是多一层解压；用户明确要的是单文件。
- **维护 `CHANGELOG.md`** — 最强理由：版本历史跟着仓库走。否决：单版本阶段没有"历史"可维护，Release 正文已经承担同样作用；出现第二个版本再评估。

## Consequences

- **收益**：首发当天就能给用户一个可下载的 `MoeFrame.exe`，README 的第一跳直接落到 Releases，不再要求对方会 `cargo build`。
- **代价**：发布链路完全依赖本机 Windows + 已登录 `gh`，无法在 CI 复核；exe 的构建环境（工具链见 `rust-toolchain.toml`，`1.99.1`）只记录在仓库文件里，不看仓库的人拿不到这个信息。
- **后续约束**：下一个版本发布前，必须先决定是否补 `release.yml`，否则每次都要有人在 Windows 桌面上手工构建。
