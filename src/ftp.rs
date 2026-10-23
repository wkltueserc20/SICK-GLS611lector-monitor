use crate::storage::{now, Store};
use anyhow::{Context, Result};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
type Shared = Arc<Mutex<Store>>;
const LIMIT: u64 = 32 * 1024 * 1024;
struct Pair {
    id: String,
    extensions: HashSet<String>,
    time: f64,
}
type Pairs = Arc<Mutex<HashMap<(String, String), Pair>>>;

fn safe_path(cwd: &Path, arg: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !arg.contains('\\') && !arg.contains(':') && !arg.contains('\0'),
        "Invalid path"
    );
    let mut out = if arg.starts_with('/') {
        PathBuf::new()
    } else {
        cwd.to_path_buf()
    };
    for c in Path::new(arg).components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::ParentDir => {
                anyhow::ensure!(out.pop(), "Path outside root");
            }
            Component::CurDir | Component::RootDir => {}
            _ => anyhow::bail!("Invalid path"),
        }
    }
    Ok(out)
}
fn no_links(root: &Path, rel: &Path) -> Result<()> {
    let mut p = root.to_path_buf();
    for c in rel.components() {
        p.push(c);
        if let Ok(m) = fs::symlink_metadata(&p) {
            anyhow::ensure!(!m.file_type().is_symlink(), "Symlink rejected")
        }
    }
    Ok(())
}
fn reply(s: &mut TcpStream, msg: &str) -> Result<()> {
    s.write_all(format!("{msg}\r\n").as_bytes())?;
    Ok(())
}
enum Data {
    Passive(TcpListener),
    Active(SocketAddr),
}
fn data_stream(data: Data, peer: IpAddr) -> Result<TcpStream> {
    let stream = match data {
        Data::Active(a) => TcpStream::connect_timeout(&a, Duration::from_secs(10))?,
        Data::Passive(l) => {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match l.accept() {
                    Ok((s, a)) if a.ip() == peer => break s,
                    Ok(_) => continue,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        anyhow::ensure!(Instant::now() < deadline, "Data connection timeout");
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    };
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    Ok(stream)
}

pub fn start(
    listener: TcpListener,
    store: Shared,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>> {
    listener.set_nonblocking(true)?;
    let pairs: Pairs = Arc::new(Mutex::new(HashMap::new()));
    let clients = Arc::new(AtomicUsize::new(0));
    Ok(thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, peer)) => {
                    if !store
                        .lock()
                        .unwrap()
                        .cameras
                        .contains_key(&peer.ip().to_string())
                        || clients.load(Ordering::Relaxed) >= 16
                    {
                        drop(stream);
                        continue;
                    }
                    clients.fetch_add(1, Ordering::Relaxed);
                    let (s, p, c) = (store.clone(), pairs.clone(), clients.clone());
                    thread::spawn(move || {
                        if let Err(e) = session(stream, s.clone(), p) {
                            s.lock().unwrap().log(&format!("FTP {}: {e:#}", peer));
                        }
                        c.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(40))
                }
                Err(_) => break,
            }
        }
    }))
}
fn session(mut control: TcpStream, store: Shared, pairs: Pairs) -> Result<()> {
    // Accepted sockets inherit FIONBIO from the listener on Windows.
    control.set_nonblocking(false)?;
    let peer = control.peer_addr()?.ip();
    let ip = peer.to_string();
    let local = control.local_addr()?.ip();
    control.set_read_timeout(Some(Duration::from_secs(120)))?;
    control.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut input = BufReader::new(control.try_clone()?);
    let mut cwd = PathBuf::new();
    let mut username = String::new();
    let mut authenticated = false;
    let mut data: Option<Data> = None;
    let root = store.lock().unwrap().root.join("photos").join(&ip);
    fs::create_dir_all(&root)?;
    reply(&mut control, "220 ChingTech camera receiver ready")?;
    store
        .lock()
        .unwrap()
        .log(&format!("FTP session opened: {ip}"));
    loop {
        let mut bytes = Vec::new();
        let n = (&mut input).take(4097).read_until(b'\n', &mut bytes)?;
        if n == 0 {
            store.lock().unwrap().log(&format!("FTP session EOF: {ip}"));
            break;
        }
        if n > 4096 {
            reply(&mut control, "500 Command too long")?;
            break;
        }
        let line = String::from_utf8_lossy(&bytes);
        let line = line.trim_end_matches(['\r', '\n']);
        let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
        let cmd = cmd.to_ascii_uppercase();
        store
            .lock()
            .unwrap()
            .log(&format!("FTP {ip} command {cmd}"));
        match cmd.as_str() {
            "USER" => {
                username = arg.into();
                authenticated = false;
                reply(&mut control, "331 Password required")?;
                continue;
            }
            "PASS" => {
                authenticated = username == "anonymous" && arg == "anonymous";
                reply(
                    &mut control,
                    if authenticated {
                        "230 Logged in"
                    } else {
                        "530 Incorrect credentials"
                    },
                )?;
                continue;
            }
            "QUIT" => {
                reply(&mut control, "221 Goodbye")?;
                break;
            }
            "FEAT" => {
                reply(
                    &mut control,
                    "211-Features\r\n UTF8\r\n EPSV\r\n PASV\r\n SIZE\r\n MDTM\r\n211 End",
                )?;
                continue;
            }
            "SYST" => {
                reply(&mut control, "215 UNIX Type: L8")?;
                continue;
            }
            "NOOP" => {
                reply(&mut control, "200 OK")?;
                continue;
            }
            _ => {}
        }
        if !authenticated {
            reply(&mut control, "530 Login required")?;
            continue;
        }
        match cmd.as_str() {
            "TYPE" | "OPTS" | "MODE" | "STRU" | "ALLO" => reply(&mut control, "200 OK")?,
            "PWD" | "XPWD" => reply(
                &mut control,
                &format!("257 \"/{}\"", cwd.to_string_lossy().replace('\\', "/")),
            )?,
            "CWD" | "XCWD" | "CDUP" => {
                let arg = if cmd == "CDUP" { ".." } else { arg };
                match safe_path(&cwd, arg) {
                    Ok(rel) if no_links(&root, &rel).is_ok() && root.join(&rel).is_dir() => {
                        cwd = rel;
                        reply(&mut control, "250 Directory changed")?
                    }
                    _ => reply(&mut control, "550 Directory unavailable")?,
                }
            }
            "MKD" | "XMKD" => match safe_path(&cwd, arg) {
                Ok(rel) if no_links(&root, &rel).is_ok() => {
                    match fs::create_dir_all(root.join(rel)) {
                        Ok(_) => reply(&mut control, "257 Directory created")?,
                        Err(_) => reply(&mut control, "550 Cannot create directory")?,
                    }
                }
                _ => reply(&mut control, "550 Invalid path")?,
            },
            "PASV" | "EPSV" => {
                data = None;
                let mut passive = None;
                for port in 30000..=30010 {
                    if let Ok(l) = TcpListener::bind(SocketAddr::new(local, port)) {
                        l.set_nonblocking(true)?;
                        passive = Some((l, port));
                        break;
                    }
                }
                if let Some((l, port)) = passive {
                    data = Some(Data::Passive(l));
                    if cmd == "EPSV" {
                        reply(
                            &mut control,
                            &format!("229 Entering Extended Passive Mode (|||{port}|)"),
                        )?
                    } else if let IpAddr::V4(addr) = local {
                        let o = addr.octets();
                        reply(
                            &mut control,
                            &format!(
                                "227 Entering Passive Mode ({},{},{},{},{},{})",
                                o[0],
                                o[1],
                                o[2],
                                o[3],
                                port / 256,
                                port % 256
                            ),
                        )?
                    }
                } else {
                    reply(&mut control, "425 No passive ports available")?
                }
            }
            "PORT" => {
                let nums: Option<Vec<u8>> = arg.split(',').map(|s| s.parse().ok()).collect();
                if let Some(v) = nums.filter(|v| v.len() == 6) {
                    let addr = IpAddr::V4(Ipv4Addr::new(v[0], v[1], v[2], v[3]));
                    let port = u16::from(v[4]) * 256 + u16::from(v[5]);
                    if addr == peer && port >= 1024 {
                        data = Some(Data::Active(SocketAddr::new(addr, port)));
                        reply(&mut control, "200 Active data connection configured")?
                    } else {
                        reply(&mut control, "501 Data address rejected")?
                    }
                } else {
                    reply(&mut control, "501 Invalid PORT")?
                }
            }
            "STOR" => {
                let rel = match safe_path(&cwd, arg) {
                    Ok(p) if no_links(&root, &p).is_ok() => p,
                    _ => {
                        reply(&mut control, "550 Invalid path")?;
                        continue;
                    }
                };
                let ext = rel
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !["jpg", "jpeg", "png", "bmp", "xml"].contains(&ext.as_str()) {
                    reply(&mut control, "550 Only camera images and XML accepted")?;
                    continue;
                }
                let Some(endpoint) = data.take() else {
                    reply(&mut control, "425 Use PASV or PORT first")?;
                    continue;
                };
                reply(&mut control, "150 Ready for image transfer")?;
                let result = (|| -> Result<()> {
                    let mut stream = data_stream(endpoint, peer)?;
                    let key = (
                        ip.clone(),
                        rel.with_extension("").to_string_lossy().to_string(),
                    );
                    let id = {
                        let mut pending = pairs.lock().unwrap();
                        pending.retain(|_, p| now() - p.time < 120.);
                        let p = pending.entry(key).or_insert_with(|| Pair {
                            id: uuid::Uuid::new_v4().to_string(),
                            extensions: HashSet::new(),
                            time: now(),
                        });
                        if p.extensions.contains(&ext) {
                            *p = Pair {
                                id: uuid::Uuid::new_v4().to_string(),
                                extensions: HashSet::new(),
                                time: now(),
                            };
                        }
                        p.extensions.insert(ext.clone());
                        p.id.clone()
                    };
                    let path = root.join(format!("{id}.{ext}"));
                    let temp = path.with_extension(format!("{ext}.part"));
                    let receive = (|| -> Result<()> {
                        let mut output = fs::File::create(&temp)?;
                        let received =
                            std::io::copy(&mut (&mut stream).take(LIMIT + 1), &mut output)?;
                        anyhow::ensure!(received <= LIMIT, "Upload exceeds 32MB");
                        anyhow::ensure!(received > 0, "Empty upload");
                        output.sync_all()?;
                        drop(output);
                        fs::rename(&temp, &path)?;
                        Ok(())
                    })();
                    if receive.is_err() {
                        let _ = fs::remove_file(temp);
                    }
                    receive?;
                    store
                        .lock()
                        .unwrap()
                        .record(&ip, &path)
                        .context("Processing uploaded file")?;
                    Ok(())
                })();
                match result {
                    Ok(()) => reply(&mut control, "226 Transfer complete")?,
                    Err(e) => {
                        store
                            .lock()
                            .unwrap()
                            .log(&format!("Upload failed {ip}: {e:#}"));
                        reply(&mut control, "451 Transfer failed; see receiver log")?
                    }
                }
            }
            "SIZE" | "MDTM" => {
                let path = safe_path(&cwd, arg)
                    .ok()
                    .filter(|p| no_links(&root, p).is_ok())
                    .map(|p| root.join(p));
                if let Some(p) = path.filter(|p| p.is_file()) {
                    let size = fs::metadata(p)?.len();
                    let message = if cmd == "SIZE" {
                        format!("213 {size}")
                    } else {
                        "213 20000101000000".into()
                    };
                    reply(&mut control, &message)?
                } else {
                    reply(&mut control, "550 File not found")?
                }
            }
            "LIST" | "NLST" | "MLSD" => {
                if let Some(endpoint) = data.take() {
                    reply(&mut control, "150 Directory listing")?;
                    match data_stream(endpoint, peer) {
                        Ok(mut s) => {
                            s.write_all(b"")?;
                            drop(s);
                            reply(&mut control, "226 Listing complete")?
                        }
                        Err(_) => reply(&mut control, "425 Data connection failed")?,
                    }
                } else {
                    reply(&mut control, "425 Use PASV first")?
                }
            }
            "ABOR" => {
                data = None;
                reply(&mut control, "226 Aborted")?
            }
            _ => reply(&mut control, "502 Command not supported")?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_rejected() {
        assert!(safe_path(Path::new(""), "../secret").is_err());
        assert!(safe_path(Path::new(""), "C:/secret").is_err());
        assert_eq!(
            safe_path(Path::new("sub"), "../file.jpg").unwrap(),
            Path::new("file.jpg")
        );
    }
}
