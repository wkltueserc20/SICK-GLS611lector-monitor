use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub fn image(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|v| v.to_str()),
        Some("jpg" | "jpeg" | "png" | "bmp")
    )
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CameraConfig {
    pub ip: String,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ftp_host: String,
    pub ftp_port: u16,
    pub web_port: u16,
    pub max_photos: usize,
    pub cameras: Vec<CameraConfig>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            ftp_host: "0.0.0.0".into(),
            ftp_port: 2121,
            web_port: 8080,
            max_photos: 1000,
            cameras: ["左上", "右上", "左下", "右下"]
                .iter()
                .enumerate()
                .map(|(i, s)| CameraConfig {
                    ip: format!("192.168.12.{}", 17 + i),
                    label: (*s).into(),
                })
                .collect(),
        }
    }
}
impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let file = root.join("config.json");
        let cfg: Self = if file.exists() {
            serde_json::from_slice(&fs::read(&file)?)?
        } else {
            Self::default()
        };
        anyhow::ensure!(
            cfg.max_photos > 0 && cfg.max_photos <= 100000,
            "照片上限設定無效"
        );
        anyhow::ensure!(cfg.cameras.len() == 4, "需要四個相機 IP");
        let mut ips = std::collections::HashSet::new();
        for c in &cfg.cameras {
            c.ip.parse::<std::net::Ipv4Addr>()
                .context("相機 IP 格式錯誤")?;
            anyhow::ensure!(ips.insert(&c.ip), "相機 IP 重複");
        }
        if !file.exists() {
            fs::write(file, serde_json::to_vec_pretty(&cfg)?)?;
        }
        Ok(cfg)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub file: String,
    pub name: String,
    pub time: f64,
    #[serde(default = "unknown")]
    pub result: String,
    #[serde(default)]
    pub codes: Vec<String>,
}
fn unknown() -> String {
    "unknown".into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Camera {
    pub ip: String,
    pub label: String,
    pub latest: Option<Snapshot>,
    #[serde(default)]
    pub ng: Option<Snapshot>,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub retained: usize,
}
pub struct Store {
    pub root: PathBuf,
    pub config: Config,
    pub cameras: BTreeMap<String, Camera>,
}

fn walk(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let p = entry?.path();
        let meta = fs::symlink_metadata(&p)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            walk(&p, out)?
        } else if image(&p) {
            out.push(p)
        }
    }
    Ok(())
}
pub fn classify(xml: &str) -> (String, Vec<String>) {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return (unknown(), vec![]);
    };
    let Some(im) = doc.descendants().find(|n| n.has_tag_name("image")) else {
        return (unknown(), vec![]);
    };
    let noread = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .eq_ignore_ascii_case("noread")
    };
    let results: Vec<_> = im
        .descendants()
        .filter(|n| {
            n.has_tag_name("result") && n.parent().is_some_and(|p| p.has_tag_name("symbol"))
        })
        .map(|n| n.text().unwrap_or("").trim())
        .collect();
    let codes: Vec<_> = results
        .iter()
        .filter(|s| !s.is_empty() && !noread(s))
        .map(|s| s.to_string())
        .collect();
    if !codes.is_empty() {
        return ("ok".into(), codes);
    }
    if im
        .descendants()
        .any(|n| noread(n.attribute("string").unwrap_or("")) || noread(n.text().unwrap_or("")))
    {
        return ("ng".into(), vec![]);
    }
    let processed = im.descendants().any(|n| {
        (n.has_tag_name("EVTimingAnalysis") && n.attribute("Name") == Some("DecoRun"))
            || (n.has_tag_name("box") && n.attribute("name") == Some("ImageProcessedByDecoder"))
            || (n.has_tag_name("text") && n.attribute("name") == Some("DPMAbort"))
    });
    let zero = im.descendants().any(|n| {
        n.has_tag_name("FramesDecoded")
            && n.parent().is_some_and(|p| p.has_tag_name("procd-img-trig"))
            && n.text().unwrap_or("").trim() == "0"
    });
    (
        if processed || zero {
            "ng".into()
        } else {
            unknown()
        },
        vec![],
    )
}
fn mtime(p: &Path) -> f64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0., |t| t.as_secs_f64())
}
fn replace_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    // Windows cannot rename over an existing destination using std::fs::rename.
    // Keep the previous state as a recovery copy until the new state is in place.
    let backup = path.with_extension("bak");
    if backup.exists() {
        fs::remove_file(&backup)?
    }
    if path.exists() {
        fs::rename(path, &backup)?
    }
    if let Err(e) = fs::rename(&temp, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(e.into());
    }
    if backup.exists() {
        fs::remove_file(backup)?
    }
    Ok(())
}
impl Store {
    pub fn open(root: PathBuf, config: Config) -> Result<Self> {
        fs::create_dir_all(root.join("data"))?;
        fs::create_dir_all(root.join("logs"))?;
        let data = root.join("data/state.json");
        let backup = data.with_extension("bak");
        let saved: BTreeMap<String, Camera> = fs::read(&data)
            .or_else(|_| fs::read(&backup))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let cameras = config
            .cameras
            .iter()
            .map(|c| {
                let mut cam = saved.get(&c.ip).cloned().unwrap_or(Camera {
                    ip: c.ip.clone(),
                    label: c.label.clone(),
                    latest: None,
                    ng: None,
                    count: 0,
                    retained: 0,
                });
                cam.label = c.label.clone();
                (c.ip.clone(), cam)
            })
            .collect();
        let mut s = Self {
            root,
            config,
            cameras,
        };
        let ips: Vec<_> = s.cameras.keys().cloned().collect();
        for ip in ips {
            fs::create_dir_all(s.root.join("photos").join(&ip))?;
            let mut files = s.files(&ip)?;
            files.sort_by(|a, b| mtime(a).total_cmp(&mtime(b)));
            for p in files {
                s.apply(&ip, &p)?;
            }
            s.prune(&ip)?;
        }
        s.save()?;
        Ok(s)
    }
    pub fn files(&self, ip: &str) -> Result<Vec<PathBuf>> {
        let mut v = vec![];
        walk(&self.root.join("photos").join(ip), &mut v)?;
        walk(&self.root.join("ng_photos").join(ip), &mut v)?;
        Ok(v)
    }
    pub fn save(&self) -> Result<()> {
        replace_bytes(
            &self.root.join("data/state.json"),
            &serde_json::to_vec_pretty(&self.cameras)?,
        )
    }
    pub fn log(&self, msg: &str) {
        use std::io::Write;
        let p = self.root.join("logs/receiver-rust.log");
        if fs::metadata(&p)
            .map(|m| m.len() > 2_000_000)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(p.with_extension("old"));
            let _ = fs::rename(&p, p.with_extension("old"));
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{:.3} {}", now(), msg);
        }
    }
    pub fn record(&mut self, ip: &str, path: &Path) -> Result<()> {
        if image(path) {
            let ext = path.extension().unwrap().to_string_lossy();
            let name = format!("latest-{}.{}", ip.rsplit('.').next().unwrap(), ext);
            replace_bytes(&self.root.join("data").join(&name), &fs::read(path)?)?;
            let cam = self.cameras.get_mut(ip).context("Unknown IP")?;
            cam.count += 1;
            cam.latest = Some(Snapshot {
                file: name,
                name: path.file_name().unwrap().to_string_lossy().into(),
                time: now(),
                result: unknown(),
                codes: vec![],
            });
            self.apply(ip, path)?;
        } else if path.extension().is_some_and(|e| e == "xml") {
            for ext in ["jpg", "jpeg", "png", "bmp"] {
                let p = path.with_extension(ext);
                if p.exists() {
                    self.apply(ip, &p)?;
                    break;
                }
            }
        }
        self.prune(ip)?;
        self.save()?;
        self.log(&format!("Received {} {}", ip, path.display()));
        Ok(())
    }
    pub fn apply(&mut self, ip: &str, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let xml = fs::read_to_string(path.with_extension("xml")).unwrap_or_default();
        let (result, codes) = classify(&xml);
        let name = path
            .file_name()
            .context("Filename")?
            .to_string_lossy()
            .to_string();
        let cam = self.cameras.get_mut(ip).context("Unknown IP")?;
        if let Some(latest) = &mut cam.latest {
            if latest.name == name {
                latest.result = result.clone();
                latest.codes = codes;
            }
        }
        if result != "ng" {
            return Ok(());
        }
        let received = mtime(path);
        if cam.ng.as_ref().is_none_or(|n| received > n.time) {
            let filename = format!(
                "ng-{}.{}",
                ip.rsplit('.').next().unwrap(),
                path.extension().unwrap().to_string_lossy()
            );
            replace_bytes(&self.root.join("data").join(&filename), &fs::read(path)?)?;
            cam.ng = Some(Snapshot {
                file: filename,
                name: name.clone(),
                time: received,
                result: "ng".into(),
                codes: vec![],
            });
        }
        let ngroot = self.root.join("ng_photos").join(ip);
        if !path.starts_with(&ngroot) {
            let relative = path.strip_prefix(self.root.join("photos").join(ip))?;
            let mut dest = ngroot.join(relative);
            fs::create_dir_all(dest.parent().unwrap())?;
            if dest.exists() || dest.with_extension("xml").exists() {
                dest.set_file_name(format!(
                    "{}_{}.{}",
                    path.file_stem().unwrap().to_string_lossy(),
                    uuid::Uuid::new_v4(),
                    path.extension().unwrap().to_string_lossy()
                ));
            }
            fs::rename(path, &dest)?;
            let meta = path.with_extension("xml");
            if meta.exists() {
                if let Err(e) = fs::rename(&meta, dest.with_extension("xml")) {
                    let _ = fs::rename(&dest, path);
                    return Err(e.into());
                }
            }
            if dest.file_name() != path.file_name() {
                let new = dest.file_name().unwrap().to_string_lossy().to_string();
                if let Some(x) = &mut cam.latest {
                    if x.name == name {
                        x.name = new.clone()
                    }
                }
                if let Some(x) = &mut cam.ng {
                    if x.name == name {
                        x.name = new
                    }
                }
            }
        }
        Ok(())
    }
    pub fn prune(&mut self, ip: &str) -> Result<()> {
        let mut files = self.files(ip)?;
        files.sort_by(|a, b| mtime(b).total_cmp(&mtime(a)).then_with(|| b.cmp(a)));
        for p in files.iter().skip(self.config.max_photos) {
            fs::remove_file(p)?;
            let xml = p.with_extension("xml");
            if xml.exists() {
                fs::remove_file(xml)?;
            }
        }
        self.cameras.get_mut(ip).unwrap().retained = files.len().min(self.config.max_photos);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_read_not_profiler() {
        let x="<camera><image><EVTimingAnalysis Name='DecoRun'/><DecoderProfiler><procd-img-trig><FramesDecoded>1</FramesDecoded></procd-img-trig></DecoderProfiler></image></camera>";
        assert_eq!(classify(x).0, "ng");
        assert_eq!(classify("bad").0, "unknown");
        assert_eq!(
            classify("<camera><image><symbol><result>001067</result></symbol></image></camera>").1,
            vec!["001067"]
        );
    }
    #[test]
    fn migrate_and_prune() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let cfg = Config {
            max_photos: 2,
            ..Config::default()
        };
        let ip = "192.168.12.17";
        let p = root.join("photos").join(ip);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("old.jpg"), b"image").unwrap();
        fs::write(
            p.join("old.xml"),
            "<camera><image><symbol><result>NoRead</result></symbol></image></camera>",
        )
        .unwrap();
        let mut s = Store::open(root.clone(), cfg).unwrap();
        assert!(root.join("ng_photos").join(ip).join("old.jpg").exists());
        let ng = s.cameras[ip].ng.clone().unwrap();
        for n in 0..3 {
            let f = p.join(format!("{n}.jpg"));
            fs::write(&f, b"new").unwrap();
            s.record(ip, &f).unwrap();
        }
        assert_eq!(s.files(ip).unwrap().len(), 2);
        assert_eq!(s.cameras[ip].ng.as_ref().unwrap().name, ng.name);
        assert!(root.join("data").join(ng.file).exists());
        drop(s);
        let _ = fs::remove_dir_all(root);
    }
}
