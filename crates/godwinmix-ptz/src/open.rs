//! Open a driver from a protocol slug and an address, which is all a person
//! types to add a camera.

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use crate::error::{Code, Error, Result};
use crate::{canon, fake, onvif, visca, Ptz};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    ViscaIp,
    ViscaTcp,
    Onvif,
    Canon,
    Fake,
}

impl Protocol {
    pub const ALL: [Protocol; 5] = [Protocol::ViscaIp, Protocol::ViscaTcp, Protocol::Onvif, Protocol::Canon, Protocol::Fake];

    pub fn slug(self) -> &'static str {
        match self {
            Protocol::ViscaIp => "visca-ip",
            Protocol::ViscaTcp => "visca-tcp",
            Protocol::Onvif => "onvif",
            Protocol::Canon => "canon",
            Protocol::Fake => "fake",
        }
    }

    pub fn parse(s: &str) -> Result<Protocol> {
        Protocol::ALL.into_iter().find(|p| p.slug() == s).ok_or_else(|| {
            let all: Vec<&str> = Protocol::ALL.iter().map(|p| p.slug()).collect();
            Error::new(Code::BadArgument, format!("'{s}' is not a PTZ protocol. Use one of: {}.", all.join(", ")))
        })
    }

    /// The port used when the address names none.
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::ViscaIp => visca::ip::PORT,
            Protocol::ViscaTcp => 5678,
            Protocol::Onvif | Protocol::Canon | Protocol::Fake => 80,
        }
    }
}

/// Everything needed to reach one camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub protocol: Protocol,
    /// `192.0.2.20`, `192.0.2.20:52381`, or for ONVIF a device service URL.
    pub address: String,
    pub user: String,
    pub password: String,
    /// The ONVIF media profile to steer; the first when not given.
    pub profile: Option<String>,
    pub timeout: Duration,
}

impl Target {
    pub fn new(protocol: Protocol, address: &str) -> Target {
        Target { protocol, address: address.to_string(), user: String::new(), password: String::new(), profile: None, timeout: Duration::from_millis(800) }
    }

    fn socket(&self) -> Result<SocketAddr> {
        let with_port = if self.address.contains(':') && !self.address.ends_with(']') { self.address.clone() } else { format!("{}:{}", self.address, self.protocol.default_port()) };
        with_port.to_socket_addrs().ok().and_then(|mut a| a.next()).ok_or_else(|| {
            Error::new(Code::BadArgument, format!("cannot resolve '{}'. Give the camera's IP address, with :port when it is not {}.", self.address, self.protocol.default_port()))
        })
    }

    /// The ONVIF device service: the address as given when it is a URL,
    /// otherwise the standard path on it.
    pub fn device_url(&self) -> String {
        if self.address.starts_with("http://") {
            self.address.clone()
        } else {
            format!("http://{}/onvif/device_service", self.address)
        }
    }
}

/// A driver for `target`. ONVIF asks the camera where its services are, so
/// it fails here when the camera is not there; the others connect on the
/// first command.
pub fn open(target: &Target) -> Result<Box<dyn Ptz>> {
    Ok(match target.protocol {
        Protocol::ViscaIp => Box::new(visca::Visca::over_ip(target.socket()?, visca::Ranges::default(), target.timeout)?),
        Protocol::ViscaTcp => Box::new(visca::Visca::over_tcp(target.socket()?, visca::Ranges::default(), target.timeout)),
        Protocol::Onvif => {
            let login = onvif::Login { user: target.user.clone(), password: target.password.clone() };
            Box::new(onvif::Onvif::connect(&target.device_url(), login, target.profile.as_deref(), target.timeout)?)
        }
        Protocol::Canon => Box::new(canon::Canon::new(&target.address, &target.user, &target.password, canon::Ranges::default(), target.timeout)),
        Protocol::Fake => Box::new(fake::FakePtz(fake::shared())),
    })
}
