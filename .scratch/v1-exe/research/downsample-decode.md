# 降采样解码

核对日：2026-11-16。只陈述 image crate 及其当时解析到的依赖能做什么，不选备选解码器。

票面问题：开启 jpeg / png / webp / bmp 后，能否在解码过程中降到约 480×320 物理像素，而不是先按原图全分辨率解码再长期持有。

## 结论

不能。image 0.25.10 的 ImageReader::decode、四个格式的 ImageDecoder::read_image，以及 DynamicImage::thumbnail，都没有「按目标尺寸边解码边缩小」的参数。Limits 超限时返回 ImageError::Limits，不缩小。thumbnail 作用在已经解码的整图上。

1080p、2K、4K、100MP 若最终只要约 480×320，解码峰值不由 480×320 决定，而由原图全分辨率输出缓冲，加上各格式的中间缓冲决定。库和文档没有这四档的实测峰值字节数，具体数字未知。

丢掉上一张 DynamicImage 后，crate 本身没有把像素留在全局缓存里的已知障碍。损坏文件和伪装扩展名多数返回 ImageError；少数路径会 panic!，调用方拿不到 Error。是否把进程打死，取决于 panic 策略和分配失败；源码没有给出「一定 abort」的保证，也没有给出「一定可恢复」的保证。

## 1. 当前 stable 与依赖

crates.io 的 max_stable_version、max_version、newest_version 都是 0.25.10，updated_at 为 2026-03-10T16:29:18.250581Z。2026-11-16 复查仍是这个版本。

来源：https://crates.io/api/v1/crates/image

发布包 Cargo.toml 与 GitHub tag v0.25.10 的依赖声明一致。crates.io 规范化后的 Cargo.toml 长度不同，但解码器 .rs 与 tag 逐文件哈希一致。

- jpeg = zune-core 0.5.0 + zune-jpeg 0.5.5。Cargo 语义是 ^，不是精确 pin。
- png = png 0.18.0
- webp = image-webp 0.2.0
- bmp 无额外 crate

来源：https://github.com/image-rs/image/blob/v0.25.10/Cargo.toml
发布包：https://static.crates.io/crates/image/image-0.25.10.crate

2026-11-16 crates.io 上满足这些要求的最新 stable：

- zune-jpeg 0.5.15（0.5.16 只有 rc）
- zune-core 0.5.3
- png 0.18.1
- image-webp 0.2.4

中间缓冲和 panic 的依赖事实，核对的是这四个最新兼容版本，不是 image 声明里的最低版本。下游 lockfile 若钉在更旧的兼容版本，依赖内部行为可以不同。image 自己的解码器包装没有把目标尺寸传下去，这一点不随依赖补丁版本改变。

来源：https://crates.io/api/v1/crates/zune-jpeg
https://crates.io/api/v1/crates/zune-core
https://crates.io/api/v1/crates/png
https://crates.io/api/v1/crates/image-webp

## 2. 分格式：能否按目标尺寸降采样解码

四个格式都不能。docs.rs 0.25.10 上，JpegDecoder、PngDecoder、WebPDecoder、BmpDecoder 的方法列表里没有 scale、set_scale，也没有按输出宽高解码的方法。

来源：
https://docs.rs/image/0.25.10/image/codecs/jpeg/struct.JpegDecoder.html
https://docs.rs/image/0.25.10/image/codecs/png/struct.PngDecoder.html
https://docs.rs/image/0.25.10/image/codecs/webp/struct.WebPDecoder.html
https://docs.rs/image/0.25.10/image/codecs/bmp/struct.BmpDecoder.html

### jpeg

JpegDecoder::new 用 read_to_end 把整个流读进 input: Vec<u8>，再 decode_headers。read_image 要求调用方缓冲长度等于 total_bytes()，然后 zune_jpeg::JpegDecoder::decode_into 写入这块全尺寸缓冲。set_limits 只做 check_dimensions 并保存 Limits。new_zune_decoder 只设置 set_max_width、set_max_height 和 jpeg_set_out_colorspace，没有输出缩放。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/jpeg/decoder.rs

zune-core 0.5.3 的 DecoderOptions 公开设置项包括 set_max_width、jpeg_set_max_scans、jpeg_set_out_colorspace。源码里的 down-sampled component 指文件里已有的色度抽样，不是调用方请求的输出尺寸。

来源：https://docs.rs/crate/zune-core/0.5.3/source/src/options/decoder.rs
https://docs.rs/crate/zune-jpeg/0.5.15/source/src/decoder.rs

### png

PngDecoder::read_image 先 assert_eq! 缓冲长度等于 total_bytes()，再 reader.next_frame(buf)，写入全尺寸缓冲。set_transformations(png::Transformations::EXPAND) 旁边的注释说的是位深：png crate 默认会把 16 bpc 缩到 8 bpc，EXPAND 把低于 8 的位深扩到 8。这不是空间降采样。

APNG 文档里的 thumbnail 是「默认图不算动画帧时，常被当成缩略图」。源码有未实现注释：TODO thumbnail(&mut self) -> Option<impl ImageDecoder>。不是按窗口尺寸解码。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/png.rs
https://docs.rs/image/0.25.10/image/codecs/png/struct.PngDecoder.html

### webp

image 的 WebPDecoder 把 read_image 转给 image_webp::WebPDecoder::read_image，没有覆盖 set_limits，因此只用 trait 默认实现：检查宽高，不缩小。image-webp 0.2.4 有 set_memory_limit，默认 memory_limit 为 usize::MAX；image 的包装没有调用它。

read_image 按原图尺寸写调用方缓冲。无损且无 alpha 时，先分配 vec![0; width * height * 4] 再拷到 RGB 输出。有损路径调用 Vp8Decoder::decode_frame，再 fill_rgb 或 fill_rgba。没有目标尺寸参数。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/webp/decoder.rs
https://docs.rs/crate/image-webp/0.2.4/source/src/decoder.rs

### bmp

BmpDecoder::read_image 把整图写入调用方缓冲。ImageDecoderRect::read_rect 是裁一块矩形，不是缩到目标尺寸。ImageReader::decode 不走 read_rect。MAX_WIDTH_HEIGHT = 0xFFFF 是拒绝过大图，不是缩小。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/bmp/decoder.rs
https://docs.rs/image/0.25.10/image/codecs/bmp/struct.BmpDecoder.html

### 解码之后的 thumbnail 与 Limits

DynamicImage::thumbnail 和 thumbnail_exact 调用 imageops::thumbnail。文档写明：对已经提供的图像做整数缩小，每个源像素只贡献给一个目标像素。它要求整图已经在内存里。

来源：https://docs.rs/image/0.25.10/image/enum.DynamicImage.html#method.thumbnail
https://docs.rs/image/0.25.10/image/imageops/fn.thumbnail.html
https://github.com/image-rs/image/blob/v0.25.10/src/images/dynimage.rs

Limits 的严格项是 max_image_width 和 max_image_height。max_alloc 默认 Some(512 * 1024 * 1024)，文档称 512MiB，非严格，部分解码器可以忽略。超限返回 ImageError::Limits，不改输出尺寸。ImageReader::decode 先 limits.reserve(decoder.total_bytes())，再 set_limits，再 DynamicImage::from_decoder。total_bytes() 是 width * height * bytes_per_pixel，用的是解码器报告的原图尺寸。

make_decoder 注释：除 PNG 外，构造时忽略 limits，之后用 ImageDecoder::set_limits。PNG 在构造时把 max_alloc 传给 png::Limits { bytes }。PNG 的 set_limits 之后有 TODO：还没有 png::Reader::change_limits()，所以构造之后再改 limits 不会收紧 png crate 的内部缓冲上限。

来源：https://docs.rs/image/0.25.10/image/struct.Limits.html
https://github.com/image-rs/image/blob/v0.25.10/src/io/limits.rs
https://github.com/image-rs/image/blob/v0.25.10/src/io/image_reader_type.rs
https://github.com/image-rs/image/blob/v0.25.10/src/io/decoder.rs

## 3. 峰值内存由什么决定

480x320 不是这四个解码器的输入。ImageReader::decode 成功返回时，持有的是全分辨率 DynamicImage。峰值至少包括这块输出缓冲；各格式在它之外还有中间缓冲。1080p、2K、4K、100MP 的实测峰值字节数，docs.rs 和上述源码都没有，未知。下面只写源码里能看到的决定因素。

输出缓冲大小由 ImageDecoder::total_bytes 决定：dimensions().0 * dimensions().1 * color_type().bytes_per_pixel()。decoder_to_vec 按这个长度分配，注释写明内存不够时 panic。ImageReader::decode 在这次分配之前先 reserve(total_bytes)；超过 max_alloc 时返回 ImageError::Limits，不分配这块输出。reserve 本身不分配，也不把图缩小。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/io/decoder.rs
https://github.com/image-rs/image/blob/v0.25.10/src/io/free_functions.rs
https://github.com/image-rs/image/blob/v0.25.10/src/io/image_reader_type.rs

jpeg：JpegDecoder 在像素解码之前就持有整个压缩文件的 Vec<u8>。ImageReader::decode 会消费 decoder，返回后这份 Vec 随 decoder 一起丢。若调用方改用 into_decoder 并留着 JpegDecoder，压缩字节会一直留到那个 decoder 被 drop。set_limits 不把 max_alloc 传给 zune，只传最大宽高。zune 在 decode_into 期间的额外缓冲大小，0.25.10 文档和本次核对的包装层没有按 1080p、2K、4K、100MP 给出数字，未知。能确定的是：输出缓冲仍是全分辨率，不是 480x320。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/jpeg/decoder.rs

png：头通过 read_header_info 读，不是 image 包装层先把整文件拷进 Vec。像素写入调用方的全尺寸缓冲。png 0.18.1 自己的 Limits 默认 bytes 为 1024 * 1024 * 64。image 在构造时用 max_alloc 覆盖它（默认 512MiB；Limits::no_limits() 时是 usize::MAX）。UnfilteringBuffer::start_frame 只记录 frame_bytes，不按整帧分配。png 中间分配的实际上限，以及它和输出缓冲是否同时存在，源码没有给出这四档的测量值，未知。能确定的是：不会因为目标是 480x320 就少分配输出缓冲。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/png.rs
https://docs.rs/crate/png/0.18.1/source/src/decoder/mod.rs
https://docs.rs/crate/png/0.18.1/source/src/decoder/unfiltering_buffer.rs

webp：输出缓冲是全尺寸。无损无 alpha 时，源码显式再分配 width * height * 4 的临时缓冲，与输出缓冲同时存在，直到拷贝结束。有损路径先 decode_frame 得到整帧 Frame，再填入调用方缓冲。image 没有把 Limits::max_alloc 传给 set_memory_limit。这两路中间缓冲对四档分辨率的实测峰值未知。

来源：https://docs.rs/crate/image-webp/0.2.4/source/src/decoder.rs

bmp：输出缓冲是全尺寸。调色板路径另分配一行索引，不是第二张全图。没有空间缩放。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/bmp/decoder.rs

因此：四档分辨率若走 ImageReader::decode，峰值由原图像素尺寸、解码后的 ColorType、以及上面的格式中间缓冲决定，不由约 480x320 决定。默认 512MiB 只决定输出 total_bytes 超过它时返回 ImageError，不决定实际 RSS，也不覆盖全部中间缓冲。某个具体文件会不会越过 512MiB，取决于该文件的宽、高和通道字节数。1080p、2K、100MP 在源码里不是固定像素尺寸，这里不换算字节数。

## 4. 只留当前图和下一张，切走后释放上一张

image 0.25.10 的 src 里没有 lazy_static、OnceLock、thread_local 或 static mut。DynamicImage 拥有 ImageBuffer，像素在它的 Vec 里。drop 掉上一张 DynamicImage，这块 Vec 随之释放。crate 没有「切走后仍持有上一张像素」的全局缓存。

ImageReader::decode 消费 reader 和 decoder。jpeg 的整文件 Vec 在 decode 返回时随 decoder 释放。若调用方自己留着 JpegDecoder（into_decoder），压缩字节会留到那个 decoder 被 drop。这是调用方还持有 decoder，不是 crate 偷偷留图。

来源：https://github.com/image-rs/image/blob/v0.25.10/src/io/image_reader_type.rs
https://github.com/image-rs/image/blob/v0.25.10/src/images/dynimage.rs
https://github.com/image-rs/image/blob/v0.25.10/README.md

已知障碍不是 drop 不掉，而是 decode 成功后手里的对象是全分辨率图。只 drop 上一张，不能把解码峰值改成两张 480x320。

## 5. 损坏文件与伪装扩展名

ImageReader::open 只按路径扩展名设格式，注释写明此时不对已打开文件做 IO。没有格式时 decode 返回 ImageError::Unsupported。with_guessed_format 读文件头魔数，成功则替换扩展名猜测。

来源：https://docs.rs/image/0.25.10/image/struct.ImageReader.html
https://github.com/image-rs/image/blob/v0.25.10/src/io/image_reader_type.rs

伪装扩展名会把字节交给扩展名对应的解码器，而不是魔数对应的解码器。png 签名不对时，png 0.18.1 返回 FormatErrorInner::InvalidSignature，image 经 ImageError::from_png 变成 ImageError，不是这条路径上的 panic!。jpeg 头失败走 decode_headers().map_err(ImageError::from_jpeg)?，在后面的 unwrap 之前返回。源码注释说那些 unwrap 只在头解码成功之后、且 JPEG 尺寸不超过 65535 时才不会失败；头失败本身是 Error。webp 和 bmp 的 new 在头不合法时返回 ImageResult 错误。

来源：https://docs.rs/crate/png/0.18.1/source/src/decoder/stream.rs
https://github.com/image-rs/image/blob/v0.25.10/src/codecs/jpeg/decoder.rs
https://github.com/image-rs/image/blob/v0.25.10/src/codecs/webp/decoder.rs
https://github.com/image-rs/image/blob/v0.25.10/src/codecs/bmp/decoder.rs

下面这些路径在调用方拿到 Error 之前就会 panic! 或按文档 panic。本次没有跑语料，所以不写「任意损坏文件都会」或「都不会」。

- decoder_to_vec 的文档写明：内存不够解码时 panic。ImageReader::decode 会先用 max_alloc 拒绝过大的输出 total_bytes。jpeg 和 webp 的中间分配不都走这个 reserve。分配失败时 Rust 默认走分配错误钩子，那不是 ImageError。哪种分辨率会触发，源码没有测量，未知。来源：https://github.com/image-rs/image/blob/v0.25.10/src/io/free_functions.rs
- PngDecoder::read_image 和 image 的 WebPDecoder::read_image 在缓冲长度不等于 total_bytes() 时 assert_eq!。ImageReader::decode 用 decoder_to_vec 按 total_bytes() 分配，这条 assert 不是这条路径的损坏文件行为。直接调用 read_image 且长度不对则会 panic。来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/png.rs 以及 https://github.com/image-rs/image/blob/v0.25.10/src/io/decoder.rs
- png 0.18.1 的 parse_plte 不检查长度是否为 3 的倍数，块长允许范围是 3 到 768。create_rgba_palette 在剩余 1 或 2 字节时对 palette_iter[0..3] 做 copy_from_slice，这是切片越界 panic，不是 DecodingError。上游 issue 写明 0.18.1 仍会 panic，修复未发版。image 0.25.10 的 png 依赖是 ^0.18.0，2026-11-16 最新兼容版就是 0.18.1。0.18.0 本次没有单独核对。来源：https://docs.rs/crate/png/0.18.1/source/src/decoder/stream.rs 以及 https://docs.rs/crate/png/0.18.1/source/src/decoder/transform/palette.rs 以及 https://github.com/image-rs/image-png/issues/721 以及 https://github.com/image-rs/image-png/issues/694
- image-webp 0.2.4 的 vp8.rs 有损系数解码里有 panic!("unknown token")，这个分支不返回 DecodingError。read_image 的 VP8 路径会调用 Vp8Decoder::decode_frame。不是每个损坏文件都必然走到这个分支；走到时调用方拿不到 Error。同文件 panic!("invalid tree with probs") 在 const fn 里，用于静态树尺寸不匹配，不是按文件运行时的损坏路径。来源：https://docs.rs/crate/image-webp/0.2.4/source/src/vp8.rs 以及 https://docs.rs/crate/image-webp/0.2.4/source/src/decoder.rs
- BmpDecoder 的 Bitfield::read 对长度不在 1 到 8 时 panic!()。Bitfields::from_mask 会拒绝 R/G/B 掩码长度为 0，并把长度大于 8 的钳到 8；alpha 长度为 0 时不调用 read。因此零掩码这条构造路径返回 ImageError，不是这个 panic!。调色板路径在 indexed_color == true 时有 row.clone_from_slice。默认 ImageReader::decode 不调用 set_indexed_color。未合并的 PR 描述这条 indexed 路径会 panic，该修复不在 0.25.10 里。来源：https://github.com/image-rs/image/blob/v0.25.10/src/codecs/bmp/decoder.rs 以及 https://github.com/image-rs/image/pull/3124
- GitHub 上还有未合并 PR 描述 BMP read_scanline 减法溢出，以及 ICO 内嵌 BMP 高度减半后为零时 unwrap panic。0.25.10 的 src/codecs/bmp/decoder.rs 里没有 fn read_scanline，也没有 end_pos - current_pos 或 height - 1 这几处表达式。它们不能当作 0.25.10 已核对到的行为。来源：https://github.com/image-rs/image/pull/3145 以及 https://github.com/image-rs/image/pull/3041
- issue 2803 是 image 0.25.9 加载一张 PNG 时在 png.rs assert panic。该 issue 已关闭。本次没有在 0.25.10 上复现，不把它写成 0.25.10 的事实。来源：https://github.com/image-rs/image/issues/2803

伪装扩展名本身不会在选错解码器的签名检查上把进程打死：签名失败返回 ImageError。签名通过、载荷损坏时，能否在跳到下一张之前不可恢复地结束进程，取决于上面那些 panic 或分配失败路径有没有被走到。源码没有一份「所有损坏文件都返回 Error」的保证。

## 未知

- 1080p、2K、4K、100MP 在约 480x320 目标下的实测峰值字节数。源码只给出公式和中间缓冲种类，没有测量。
- zune-jpeg 0.5.15 在 baseline 或 progressive 下，除全尺寸输出和整文件 Vec 之外，峰值还要加多少字节。
- png 0.18.1 在 image 传入的 Limits.bytes 之下，中间缓冲与全尺寸输出同时存在时的峰值。
- 任意损坏的 jpeg、png、webp、bmp 是否都会命中上述 panic。只核对到有的路径返回 Error，有的路径源码是 panic!。
- png 0.18.0（依赖声明的下界）是否已有 PLTE 长度 panic。只核对了 0.18.1。
