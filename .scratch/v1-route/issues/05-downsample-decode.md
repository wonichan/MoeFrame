# 降采样解码

Label: wayfinder:research
Type: research
Status: claimed

## Question

`image` crate 开启 jpeg/png/webp/bmp 后，能否在解码过程中降到窗口物理像素尺寸，而不是先按原图全分辨率解码再长期持有。

要核对的事实：

- 当前 stable 版本号，以及 jpeg、png、webp、bmp 是否支持按目标尺寸降采样解码。
- 一张 1080p、2K、4K、100MP 的图，若目标是约 480×320 物理像素，峰值内存大致由什么决定。
- 只保留当前图和下一张、切走后释放上一张，有没有 crate 层面的已知障碍。
- 损坏文件和伪装扩展名会不会在到达「跳到下一张」之前就把进程打死。

只核对官方文档和 image-rs 源码。守不住时不要在这张票里选备选解码器。
