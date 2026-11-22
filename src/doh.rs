//! Encrypted DNS (DNS over HTTPS, RFC 8484): the names of the sites visited
//! are asked of a resolver over https instead of being sent in clear to the
//! network's own DNS, where the access provider — or anyone on the same
//! Wi-Fi — reads them.
//!
//! The web engine resolves names through the system and has no setting for
//! this. So when it is switched on, Sanaga runs a small relay on this
//! computer (SOCKS5, on 127.0.0.1, closed by a secret made at each start)
//! and gives it to the engine as its proxy: the engine hands the relay the
//! *name* of each site, the relay asks the resolver over https, connects,
//! and passes the bytes along untouched (the pages' own encryption goes
//! through it end to end). Sanaga's own requests use the same resolver.
//!
//! Left to the system: this computer itself, and the names of the home or
//! office network (`printer.local`, `nas`), which no resolver of the
//! Internet knows. When the resolver does not answer, the name is not
//! asked in clear instead: the page fails, and says so.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// The resolvers offered in the settings: name, address.
pub const RESOLVERS: [(&str, &str); 3] = [("Quad9", "https://dns.quad9.net/dns-query"), ("Cloudflare", "https://cloudflare-dns.com/dns-query"), ("Mullvad", "https://dns.mullvad.net/dns-query")];

/// An answer is kept at least this long and at most that long, whatever it says.
const KEEP: (u64, u64) = (30, 3600);
/// The most names kept.
const MAX_KEPT: usize = 2048;

// ---------------------------------------------------------------- messages

/// The question "what are the addresses of `name`?" as a DNS message
/// (`kind`: 1 for IPv4 addresses, 28 for IPv6). `None`: not a name.
pub fn question(name: &str, kind: u16) -> Option<Vec<u8>> {
    let name = name.trim_end_matches('.');
    if name.is_empty() || name.len() > 253 {
        return None;
    }
    // (id 0, as the RFC asks: the same question gives the same bytes)
    let mut m = vec![0, 0, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 || !label.is_ascii() {
            return None;
        }
        m.push(label.len() as u8);
        m.extend_from_slice(label.as_bytes());
    }
    m.push(0);
    m.extend_from_slice(&kind.to_be_bytes());
    m.extend_from_slice(&[0, 1]);
    Some(m)
}

/// Past a name at `at` in a message: where what follows it starts.
fn after_name(m: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let len = *m.get(at)? as usize;
        if len & 0xC0 == 0xC0 {
            // (a pointer to a name said earlier: two bytes, and the name ends)
            m.get(at + 1)?;
            return Some(at + 2);
        }
        if len & 0xC0 != 0 {
            return None;
        }
        at += 1 + len;
        if len == 0 {
            return Some(at);
        }
    }
}

/// The addresses in a DNS answer, and for how long they may be kept
/// (seconds). `None`: not an answer that can be read; an empty list: the
/// name has no address of that kind (or does not exist).
pub fn addresses(m: &[u8]) -> Option<(Vec<IpAddr>, u64)> {
    if m.len() < 12 || m[2] & 0x80 == 0 {
        return None;
    }
    let (questions, answers) = (u16::from_be_bytes([m[4], m[5]]), u16::from_be_bytes([m[6], m[7]]));
    let mut at = 12;
    for _ in 0..questions {
        at = after_name(m, at)?.checked_add(4)?;
    }
    let mut found = Vec::new();
    let mut keep = u64::MAX;
    for _ in 0..answers {
        at = after_name(m, at)?;
        let head = m.get(at..at + 10)?;
        let kind = u16::from_be_bytes([head[0], head[1]]);
        let ttl = u32::from_be_bytes([head[4], head[5], head[6], head[7]]) as u64;
        let len = u16::from_be_bytes([head[8], head[9]]) as usize;
        let data = m.get(at + 10..at + 10 + len)?;
        at += 10 + len;
        match (kind, len) {
            (1, 4) => found.push(IpAddr::V4(Ipv4Addr::new(data[0], data[1], data[2], data[3]))),
            (28, 16) => found.push(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(data).ok()?))),
            // (another name for the same site, a signature…: passed over)
            _ => continue,
        }
        keep = keep.min(ttl);
    }
    Some((found, if keep == u64::MAX { 0 } else { keep }))
}

/// Is this a name for the system to resolve, not a resolver of the
/// Internet: this computer, a name of the home or office network?
pub fn local_name(name: &str) -> bool {
    let n = name.trim_end_matches('.').to_ascii_lowercase();
    let last = n.rsplit('.').next().unwrap_or("");
    !n.contains('.') || matches!(last, "local" | "localhost" | "lan" | "home" | "internal" | "intranet" | "corp" | "test" | "invalid") || n.ends_with(".home.arpa")
}

// ---------------------------------------------------------------- resolver

struct Resolver {
    address: String,
    /// (its own client: the system resolves the resolver's own name, once
    /// per connection, and the connection is kept)
    agent: ureq::Agent,
    kept: Mutex<HashMap<String, (Vec<IpAddr>, Instant)>>,
}

static RESOLVER: RwLock<Option<Arc<Resolver>>> = RwLock::new(None);

/// The setting: the resolver's address (`https://…/dns-query`), or "" for
/// the system's DNS. True when it changed.
pub fn set(address: &str) -> bool {
    let address = address.trim();
    let Ok(mut r) = RESOLVER.write() else { return false };
    if r.as_ref().map(|x| x.address.as_str()).unwrap_or("") == address {
        return false;
    }
    *r = usable(address).then(|| {
        let agent = crate::net::plain_builder().timeout_connect(Duration::from_secs(6)).timeout_read(Duration::from_secs(6)).timeout_write(Duration::from_secs(6)).build();
        Arc::new(Resolver { address: address.to_string(), agent, kept: Mutex::new(HashMap::new()) })
    });
    true
}

/// Is this an address a resolver can have (https, a name, no login)?
pub fn usable(address: &str) -> bool {
    url::Url::parse(address.trim()).is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some_and(|h| !h.is_empty()) && u.username().is_empty())
}

/// Has the resolver answered lately (within a minute)? When a name is not
/// found, this tells "no such site" from "the resolver cannot be reached".
pub fn answers() -> bool {
    ANSWERED.lock().is_ok_and(|a| a.is_some_and(|t| t.elapsed() < Duration::from_secs(60)))
}

static ANSWERED: Mutex<Option<Instant>> = Mutex::new(None);

/// Is encrypted DNS switched on?
pub fn on() -> bool {
    RESOLVER.read().is_ok_and(|r| r.is_some())
}

fn ask(r: &Resolver, name: &str, kind: u16) -> Result<(Vec<IpAddr>, u64), String> {
    let q = question(name, kind).ok_or("not a name")?;
    let answer = r.agent.post(&r.address).set("content-type", "application/dns-message").set("accept", "application/dns-message").send_bytes(&q).map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    answer.into_reader().take(65_536).read_to_end(&mut body).map_err(|e| e.to_string())?;
    let read = addresses(&body).ok_or_else(|| "an answer that cannot be read".to_string())?;
    if let Ok(mut a) = ANSWERED.lock() {
        *a = Some(Instant::now());
    }
    Ok(read)
}

/// The addresses of a site, asked over https (IPv4 first). Numbers are
/// taken as they are; local names go to the system. An error: the resolver
/// did not answer, or the name does not exist — never asked in clear instead.
pub fn lookup(name: &str) -> std::io::Result<Vec<IpAddr>> {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = name.trim_matches(['[', ']']).parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let resolver = RESOLVER.read().ok().and_then(|r| r.clone());
    let Some(r) = resolver.filter(|_| !local_name(&name)) else {
        return (name.as_str(), 0).to_socket_addrs().map(|a| a.map(|s| s.ip()).collect());
    };
    if let Some((found, until)) = r.kept.lock().ok().and_then(|k| k.get(&name).cloned()) {
        if Instant::now() < until {
            return Ok(found);
        }
    }
    // (both kinds of address at once: a site is reached by whichever works)
    let (v4, v6) = std::thread::scope(|s| {
        let six = s.spawn(|| ask(&r, &name, 28));
        (ask(&r, &name, 1), six.join().unwrap_or_else(|_| Err("stopped".into())))
    });
    let mut found = Vec::new();
    let mut keep = KEEP.1;
    let mut why = String::new();
    for answer in [v4, v6] {
        match answer {
            Ok((ips, ttl)) => {
                if !ips.is_empty() {
                    keep = keep.min(ttl);
                }
                found.extend(ips);
            }
            Err(e) => why = e,
        }
    }
    if found.is_empty() {
        let kind = if why.is_empty() { std::io::ErrorKind::NotFound } else { std::io::ErrorKind::Other };
        return Err(std::io::Error::new(kind, if why.is_empty() { format!("{name}: no such site") } else { format!("encrypted DNS: {why}") }));
    }
    if let Ok(mut k) = r.kept.lock() {
        if k.len() >= MAX_KEPT {
            k.clear();
        }
        k.insert(name, (found.clone(), Instant::now() + Duration::from_secs(keep.clamp(KEEP.0, KEEP.1))));
    }
    Ok(found)
}

/// For Sanaga's own requests (block lists, translation, updates…): the same resolver.
pub struct Own;

impl ureq::Resolver for Own {
    fn resolve(&self, netloc: &str) -> std::io::Result<Vec<SocketAddr>> {
        let (host, port) = netloc.rsplit_once(':').ok_or_else(|| std::io::Error::other("no port"))?;
        let port: u16 = port.parse().map_err(|_| std::io::Error::other("no port"))?;
        Ok(lookup(host)?.into_iter().map(|ip| SocketAddr::new(ip, port)).collect())
    }
}

// ------------------------------------------------------------------- relay

/// The most connections the relay serves at once (one small thread each).
/// More wait in line for a moment, then are refused: whatever opens them,
/// the relay never grows without bound.
const MAX_CONNECTIONS: usize = 256;
/// How long a connection waits in line for a place.
const LINE_WAIT: Duration = Duration::from_secs(10);

/// Places for connections: taken by each one served, given back when it ends.
struct Places {
    used: Mutex<usize>,
    freed: std::sync::Condvar,
    most: usize,
}

/// A place taken (given back when dropped).
struct Place(Arc<Places>);

impl Drop for Place {
    fn drop(&mut self) {
        if let Ok(mut used) = self.0.used.lock() {
            *used = used.saturating_sub(1);
        }
        self.0.freed.notify_one();
    }
}

impl Places {
    fn new(most: usize) -> Arc<Places> {
        Arc::new(Places { used: Mutex::new(0), freed: std::sync::Condvar::new(), most })
    }

    /// A place, waiting at most `wait` for one to be given back.
    fn take(places: &Arc<Places>, wait: Duration) -> Option<Place> {
        let until = Instant::now() + wait;
        let mut used = places.used.lock().ok()?;
        while *used >= places.most {
            let left = until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
            used = places.freed.wait_timeout(used, left).ok()?.0;
        }
        *used += 1;
        Some(Place(places.clone()))
    }
}

/// The relay's address as a proxy for the web engine (with its secret),
/// started at the first call. `None`: it could not be started.
pub fn relay() -> Option<String> {
    static RELAY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    RELAY
        .get_or_init(|| {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
            let port = listener.local_addr().ok()?.port();
            // (a secret no other program of this computer knows: the relay
            // is Sanaga's, not a way out for whoever finds its port)
            let mut bytes = [0u8; 16];
            std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).ok()?;
            let secret: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let known = secret.clone();
            std::thread::Builder::new()
                .name("sanaga-dns-relay".into())
                .spawn(move || {
                    let places = Places::new(MAX_CONNECTIONS);
                    for client in listener.incoming().flatten() {
                        // (all places taken: this one waits a moment — the
                        // next ones wait in the system's queue — then goes)
                        let Some(place) = Places::take(&places, LINE_WAIT) else {
                            drop(client);
                            continue;
                        };
                        let secret = known.clone();
                        // (one small thread per connection: it sleeps until bytes come)
                        let _ = std::thread::Builder::new().name("sanaga-dns-conn".into()).stack_size(128 * 1024).spawn(move || {
                            let _place = place;
                            let _ = serve(client, &secret);
                        });
                    }
                })
                .ok()?;
            Some(format!("socks5://sanaga:{secret}@127.0.0.1:{port}"))
        })
        .clone()
}

fn bytes<const N: usize>(s: &mut TcpStream) -> std::io::Result<[u8; N]> {
    let mut b = [0u8; N];
    s.read_exact(&mut b)?;
    Ok(b)
}

fn text(s: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let [len] = bytes::<1>(s)?;
    let mut b = vec![0u8; len as usize];
    s.read_exact(&mut b)?;
    Ok(b)
}

/// One connection of the engine: who it is (the secret), where it wants to
/// go (a name, or a number), then the bytes both ways until one side ends.
fn serve(mut client: TcpStream, secret: &str) -> std::io::Result<()> {
    let refuse = |_: ()| std::io::Error::other("refused");
    client.set_read_timeout(Some(Duration::from_secs(10)))?;
    // greeting: SOCKS 5, the ways it can say who it is
    let [version, ways] = bytes::<2>(&mut client)?;
    let mut offered = vec![0u8; ways as usize];
    client.read_exact(&mut offered)?;
    if version != 5 || !offered.contains(&2) {
        let _ = client.write_all(&[5, 0xFF]);
        return Err(refuse(()));
    }
    client.write_all(&[5, 2])?;
    // name and secret (RFC 1929)
    let [auth] = bytes::<1>(&mut client)?;
    let (_user, pass) = (text(&mut client)?, text(&mut client)?);
    // (compared whole, in the same time whatever differs)
    let same = pass.len() == secret.len() && pass.iter().zip(secret.as_bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0;
    if auth != 1 || !same {
        let _ = client.write_all(&[1, 1]);
        return Err(refuse(()));
    }
    client.write_all(&[1, 0])?;
    // the request: connect to a name or a number
    let [version, command, _, kind] = bytes::<4>(&mut client)?;
    let host = match kind {
        1 => IpAddr::V4(Ipv4Addr::from(bytes::<4>(&mut client)?)).to_string(),
        4 => IpAddr::V6(Ipv6Addr::from(bytes::<16>(&mut client)?)).to_string(),
        3 => String::from_utf8(text(&mut client)?).map_err(|_| refuse(()))?,
        _ => return Err(refuse(())),
    };
    let port = u16::from_be_bytes(bytes::<2>(&mut client)?);
    let answer = |code: u8| [5, code, 0, 1, 0, 0, 0, 0, 0, 0];
    if version != 5 || command != 1 {
        let _ = client.write_all(&answer(7)); // (only "connect")
        return Err(refuse(()));
    }
    let site = match lookup(&host) {
        Ok(ips) => ips.into_iter().find_map(|ip| TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_secs(10)).ok()),
        Err(e) => {
            if std::env::var_os("SANAGA_DEBUG_DNS").is_some() {
                eprintln!("encrypted DNS: {host}: {e}");
            }
            let _ = client.write_all(&answer(4)); // (host unreachable: no such name)
            return Err(refuse(()));
        }
    };
    let Some(site) = site else {
        let _ = client.write_all(&answer(5)); // (connection refused)
        return Err(refuse(()));
    };
    client.write_all(&answer(0))?;
    client.set_read_timeout(None)?;
    let _ = (client.set_nodelay(true), site.set_nodelay(true));
    pass_along(client, site)
}

/// The bytes of each side handed to the other, untouched, until both have
/// ended. The thread sleeps while nothing comes.
fn pass_along(a: TcpStream, b: TcpStream) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let sides = [&a, &b];
    let mut open = [true, true];
    let mut buf = vec![0u8; 32 * 1024];
    while open[0] || open[1] {
        let mut fds = [
            libc::pollfd { fd: a.as_raw_fd(), events: if open[0] { libc::POLLIN } else { 0 }, revents: 0 },
            libc::pollfd { fd: b.as_raw_fd(), events: if open[1] { libc::POLLIN } else { 0 }, revents: 0 },
        ];
        // SAFETY: two descriptors of sockets this function owns, for the length given.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        for k in 0..2 {
            if !open[k] || fds[k].revents == 0 {
                continue;
            }
            let (mut from, mut to) = (sides[k], sides[1 - k]);
            match from.read(&mut buf) {
                Ok(0) | Err(_) => {
                    // (this side has said everything: the other is told)
                    open[k] = false;
                    let _ = to.shutdown(std::net::Shutdown::Write);
                }
                Ok(n) => {
                    if to.write_all(&buf[..n]).is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_serves_a_bounded_number_of_connections() {
        let places = Places::new(2);
        let (a, b) = (Places::take(&places, Duration::ZERO).unwrap(), Places::take(&places, Duration::ZERO).unwrap());
        // full: refused once the wait is over
        let started = Instant::now();
        assert!(Places::take(&places, Duration::from_millis(50)).is_none());
        assert!(started.elapsed() >= Duration::from_millis(50));
        // a connection that ends gives its place back
        drop(a);
        let c = Places::take(&places, Duration::ZERO).unwrap();
        // one waiting in line gets the place given back meanwhile
        let waiting = {
            let places = places.clone();
            std::thread::spawn(move || Places::take(&places, Duration::from_secs(5)).is_some())
        };
        std::thread::sleep(Duration::from_millis(30));
        drop(b);
        assert!(waiting.join().unwrap());
        drop(c);
        assert_eq!(*places.used.lock().unwrap(), 0, "every place given back");
        assert!(MAX_CONNECTIONS >= 64 && MAX_CONNECTIONS <= 1024);
    }

    /// An answer to `question(name, kind)` with these records (kind, ttl, data).
    fn answer(name: &str, kind: u16, records: &[(u16, u32, &[u8])]) -> Vec<u8> {
        let mut m = question(name, kind).unwrap();
        m[2] |= 0x80;
        m[7] = records.len() as u8;
        for (kind, ttl, data) in records {
            m.extend_from_slice(&[0xC0, 12]); // (the name of the question)
            m.extend_from_slice(&kind.to_be_bytes());
            m.extend_from_slice(&[0, 1]);
            m.extend_from_slice(&ttl.to_be_bytes());
            m.extend_from_slice(&(data.len() as u16).to_be_bytes());
            m.extend_from_slice(data);
        }
        m
    }

    #[test]
    fn a_question_is_a_dns_message() {
        let q = question("example.org", 1).unwrap();
        assert_eq!(q, [0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'o', b'r', b'g', 0, 0, 1, 0, 1]);
        assert_eq!(question("example.org.", 28).unwrap()[q.len() - 3], 28);
        for wrong in ["", "a..b", &"x".repeat(64), "exämple.org"] {
            assert_eq!(question(wrong, 1), None, "{wrong}");
        }
    }

    #[test]
    fn the_addresses_of_an_answer_are_read() {
        let m = answer("example.org", 1, &[(5, 300, &[3, b'w', b'w', b'w', 0xC0, 12]), (1, 120, &[93, 184, 216, 34]), (1, 60, &[93, 184, 216, 35])]);
        let (ips, keep) = addresses(&m).unwrap();
        assert_eq!(ips, vec![IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), IpAddr::V4(Ipv4Addr::new(93, 184, 216, 35))]);
        assert_eq!(keep, 60);
        let six = [0x26, 0x06, 0x28, 0, 0x02, 0x20, 0, 1, 0x02, 0x48, 0x18, 0x93, 0x25, 0xc8, 0x19, 0x46];
        assert_eq!(addresses(&answer("example.org", 28, &[(28, 10, &six)])).unwrap().0, vec![IpAddr::V6(Ipv6Addr::from(six))]);
        // a name without an address of that kind
        assert_eq!(addresses(&answer("example.org", 1, &[])).unwrap(), (vec![], 0));
        // the question itself is not an answer
        assert_eq!(addresses(&question("example.org", 1).unwrap()), None);
    }

    #[test]
    fn a_damaged_answer_is_refused_not_read_past_its_end() {
        let whole = answer("example.org", 1, &[(1, 120, &[93, 184, 216, 34])]);
        // cut anywhere: no address made up, no reading past the end
        for cut in 0..whole.len() {
            assert!(addresses(&whole[..cut]).is_none_or(|(ips, _)| ips.is_empty()), "cut at {cut}");
        }
        // every byte changed in turn: never a panic
        for at in 0..whole.len() {
            for bits in [0x01u8, 0x80, 0xFF] {
                let mut m = whole.clone();
                m[at] ^= bits;
                let _ = addresses(&m);
            }
        }
        // a name that points at itself for ever
        let mut m = whole.clone();
        m[12] = 0xC0;
        m[13] = 12;
        let _ = addresses(&m);
    }

    #[test]
    fn names_of_this_computer_and_of_the_home_network_stay_with_the_system() {
        for local in ["localhost", "nas", "printer.local", "box.lan", "router.home.arpa", "app.localhost", "PC."] {
            assert!(local_name(local), "{local}");
        }
        for far in ["example.org", "www.example.co.uk", "local.example.org"] {
            assert!(!local_name(far), "{far}");
        }
        assert!(usable("https://dns.quad9.net/dns-query"));
        for wrong in ["", "http://dns.example/dns-query", "dns.quad9.net", "https://user@dns.example/q"] {
            assert!(!usable(wrong), "{wrong}");
        }
    }
}
