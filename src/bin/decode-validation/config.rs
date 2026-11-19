use moe_frame::decode_validation::{DecodeLimits, DisplayMode};
use std::{ffi::OsString, fs, path::PathBuf, time::Duration};

pub const HELP: &str = "decode-validation [options] -- <image paths>\n  --max-width N --max-height N --max-alloc-mib N  Required experimental Limits (not product defaults)\n  --mode fit|fill           Display mode (default fit)\n  --list FILE               UTF-8 image paths, relative to the list file\n  --interval-ms N           Switch delay after preparation (default 5000)\n  --sample-ms N             Windows process sampling period (default 25)\n  --duration-secs N         Auto-close after first prepared display; baseline starts at launch\n  --log-dir DIR             Default: system temp/MoeFrame-decode-validation\n  --baseline                Empty GUI only; no image paths or Limits\n  --no-system-font-fallback Diagnostic opt-out; font coverage differs, default enabled\n  --font-file FILE          Diagnostic: register this single font file as the only fallback (implies --no-system-font-fallback)\n  --help                    Show this help\n";

#[derive(Debug)]
pub struct Config {
    pub paths: Vec<PathBuf>,
    pub limits: Option<DecodeLimits>,
    pub mode: DisplayMode,
    pub interval: Duration,
    pub sample_period: Duration,
    pub duration: Option<Duration>,
    pub log_dir: PathBuf,
    pub baseline: bool,
    pub system_font_fallback: bool,
    pub font_file: Option<PathBuf>,
}

impl Config {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut args = args.into_iter();
        let mut paths = Vec::new();
        let (mut width, mut height, mut alloc) = (None, None, None);
        let mut mode = DisplayMode::Fit;
        let mut interval = Duration::from_millis(5000);
        let mut sample_period = Duration::from_millis(25);
        let mut duration = None;
        let mut log_dir = std::env::temp_dir().join("MoeFrame-decode-validation");
        let mut baseline = false;
        let mut system_font_fallback = true;
        let mut font_file = None;
        while let Some(arg) = args.next() {
            if arg == "--" {
                paths.extend(args.map(PathBuf::from));
                break;
            }
            if arg == "--help" || arg == "-h" {
                return Ok(None);
            }
            if arg == "--baseline" {
                baseline = true;
                continue;
            }
            if arg == "--no-system-font-fallback" {
                system_font_fallback = false;
                continue;
            }
            if !arg.to_string_lossy().starts_with("--") {
                paths.push(PathBuf::from(arg));
                continue;
            }
            let option = arg.to_str().ok_or("option name must be UTF-8")?;
            let value = args
                .next()
                .ok_or_else(|| format!("{option} requires a value"))?;
            match option {
                "--max-width" => {
                    width =
                        Some(u32::try_from(positive(&value, option)?).map_err(|e| e.to_string())?)
                }
                "--max-height" => {
                    height =
                        Some(u32::try_from(positive(&value, option)?).map_err(|e| e.to_string())?)
                }
                "--max-alloc-mib" => {
                    alloc = Some(
                        positive(&value, option)?
                            .checked_mul(1024 * 1024)
                            .ok_or("max_alloc byte count overflow")?,
                    )
                }
                "--mode" => {
                    mode = match value.to_str() {
                        Some("fit") => DisplayMode::Fit,
                        Some("fill") => DisplayMode::Fill,
                        _ => return Err("--mode must be fit or fill".into()),
                    }
                }
                "--interval-ms" => interval = Duration::from_millis(positive(&value, option)?),
                "--sample-ms" => sample_period = Duration::from_millis(positive(&value, option)?),
                "--duration-secs" => {
                    duration = Some(Duration::from_secs(positive(&value, option)?))
                }
                "--log-dir" => log_dir = PathBuf::from(value),
                "--font-file" => font_file = Some(PathBuf::from(value)),
                "--list" => {
                    let list = PathBuf::from(value);
                    let contents = fs::read_to_string(&list)
                        .map_err(|e| format!("{}: {e}", list.display()))?;
                    let parent = list.parent().ok_or("list path has no parent")?;
                    paths.extend(
                        contents
                            .lines()
                            .filter(|line| !line.is_empty() && !line.starts_with('#'))
                            .map(|line| parent.join(line)),
                    );
                }
                _ => return Err(format!("unknown option: {option}")),
            }
        }
        let limits = if baseline {
            if !paths.is_empty() || width.is_some() || height.is_some() || alloc.is_some() {
                return Err(
                    "--baseline cannot include images or Limits; it is an empty GUI measurement"
                        .into(),
                );
            }
            None
        } else {
            if paths.is_empty() {
                return Err(
                    "provide image paths or --list; use --baseline for an empty GUI".into(),
                );
            }
            Some(DecodeLimits {
                max_width: width.ok_or("provide explicit --max-width")?,
                max_height: height.ok_or("provide explicit --max-height")?,
                max_alloc: alloc.ok_or("provide explicit --max-alloc-mib")?,
            })
        };
        // A single registered fallback file replaces system enumeration entirely.
        if font_file.is_some() {
            system_font_fallback = false;
        }
        Ok(Some(Self {
            paths,
            limits,
            mode,
            interval,
            sample_period,
            duration,
            log_dir,
            baseline,
            system_font_fallback,
            font_file,
        }))
    }
}
fn positive(value: &OsString, option: &str) -> Result<u64, String> {
    let n = value
        .to_str()
        .ok_or_else(|| format!("{option} must be UTF-8"))?
        .parse::<u64>()
        .map_err(|e| format!("{option}: {e}"))?;
    if n == 0 {
        return Err(format!("{option} must be positive"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<Config>, String> {
        Config::parse(args.iter().map(OsString::from))
    }
    #[test]
    fn image_run_has_no_implicit_limits() {
        assert!(parse(&["a.png"])
            .unwrap_err()
            .contains("explicit --max-width"));
        let c = parse(&[
            "--max-width",
            "8192",
            "--max-height",
            "8192",
            "--max-alloc-mib",
            "96",
            "a.png",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            c.limits.unwrap(),
            DecodeLimits {
                max_width: 8192,
                max_height: 8192,
                max_alloc: 100_663_296
            }
        );
    }
    #[test]
    fn system_font_optout_is_an_explicit_diagnostic_not_the_default() {
        assert!(
            parse(&["--baseline"])
                .unwrap()
                .unwrap()
                .system_font_fallback
        );
        assert!(
            !parse(&["--baseline", "--no-system-font-fallback"])
                .unwrap()
                .unwrap()
                .system_font_fallback
        );
    }
    #[test]
    fn single_font_file_replaces_system_font_enumeration() {
        let plain = parse(&["--baseline"]).unwrap().unwrap();
        assert!(plain.font_file.is_none() && plain.system_font_fallback);
        let single = parse(&["--baseline", "--font-file", "C:/Windows/Fonts/msyh.ttc"])
            .unwrap()
            .unwrap();
        assert_eq!(
            single.font_file.as_deref(),
            Some(std::path::Path::new("C:/Windows/Fonts/msyh.ttc"))
        );
        assert!(
            !single.system_font_fallback,
            "registering one font file must not also enumerate the whole system"
        );
    }
    #[test]
    fn invalid_limits_and_mib_overflow_are_refused() {
        assert!(parse(&["--max-width", "0", "a.png"]).is_err());
        assert!(parse(&["--max-alloc-mib", "18446744073709551615", "a.png"])
            .unwrap_err()
            .contains("overflow"));
    }
    #[test]
    fn baseline_cannot_hide_image_rejections() {
        assert!(parse(&["--baseline"]).unwrap().unwrap().limits.is_none());
        assert!(parse(&["--baseline", "a.png"]).is_err());
        assert!(parse(&["--baseline", "--max-width", "1"]).is_err());
    }
}
