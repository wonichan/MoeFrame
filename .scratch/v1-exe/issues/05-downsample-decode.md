# 降采样解码

Label: wayfinder:research
Type: research
Status: resolved

## Question

`image` crate 开启 jpeg/png/webp/bmp 后，能否在解码过程中降到窗口物理像素尺寸，而不是先按原图全分辨率解码再长期持有。

要核对的事实：

- 当前 stable 版本号，以及 jpeg、png、webp、bmp 是否支持按目标尺寸降采样解码。
- 一张 1080p、2K、4K、100MP 的图，若目标是约 480×320 物理像素，峰值内存大致由什么决定。
- 只保留当前图和下一张、切走后释放上一张，有没有 crate 层面的已知障碍。
- 损坏文件和伪装扩展名会不会在到达「跳到下一张」之前就把进程打死。

只核对官方文档和 image-rs 源码。守不住时不要在这张票里选备选解码器。

正在跑的研究结论写到 [downsample-decode.md](../../v1-route/research/downsample-decode.md)。不要另起一份。

## Answer

不能。image 0.25.10 的 jpeg、png、webp、bmp 都没有按目标尺寸边解码边缩小的参数。`thumbnail` 作用在已经解码的整图上。`Limits` 超限时返回错误，不缩小。不选备选解码器。全文在 [downsample-decode.md](../../v1-route/research/downsample-decode.md)。

- 约 480×320 不是解码峰值的输入。峰值由原图全分辨率输出缓冲，加上各格式的中间缓冲决定。1080p、2K、4K、100MP 的实测峰值字节数未知。
- 丢掉上一张 `DynamicImage` 后，crate 没有把像素留在全局缓存里的已知障碍。只丢上一张，不能把解码峰值变成两张小图。
- 损坏文件和伪装扩展名多数返回 `ImageError`。少数路径会 panic，调用方拿不到 Error。没有「一定可恢复」或「一定打死进程」的保证。
