# 全分辨率解码

Label: wayfinder:grilling
Type: grilling
Status: open

## Question

image 0.25.10 不能在 jpeg、png、webp、bmp 的解码过程中降到窗口物理像素。只留当前图和下一张，也不能把解码峰值变成两张小图。1080p、2K、4K、100MP 的实测峰值字节数未知，所以还不能说已经守不住 50MB，也不能说 100MP 一定撑爆。

要决定：V1 是否仍用 image 做全分辨率解码，并用 Limits 拒绝过大的输出，而不是现在换解码器。若否，下一张票才选备选路径。

不要在这张票里选备选解码器。
