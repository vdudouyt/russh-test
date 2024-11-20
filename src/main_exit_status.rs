use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use russh::keys::*;
use russh_keys::key::KeyPair::Ed25519;
use russh::server::{Msg, Server as _, Session};
use russh::*;
use tokio::sync::Mutex;
use log::info;

static SERVER_KEY_PATH : &str = "/tmp/russh-test.key";

fn provide_server_key() -> std::io::Result<russh_keys::key::KeyPair> {
    let key = if let Ok(bytes) = std::fs::read(SERVER_KEY_PATH) {
        ed25519_dalek::SigningKey::from_bytes(&bytes.try_into().expect("invalid key length"))
    } else if let Ed25519(newkey) = russh_keys::key::KeyPair::generate_ed25519() {
        std::fs::write(SERVER_KEY_PATH, newkey.as_bytes())?;
        newkey
    } else {
        panic!("Key generation failed");
    };

    Ok(Ed25519(key))
}

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mykey = provide_server_key().unwrap();
    let config = russh::server::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(0),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        methods: russh::MethodSet::PASSWORD,
        keys: vec![ mykey ],
        ..Default::default()
    };
    let config = Arc::new(config);
    let mut sh = Server {
        clients: Arc::new(Mutex::new(HashMap::new())),
        id: 0,
    };
    sh.run_on_address(config, ("0.0.0.0", 2222)).await.unwrap();
}

#[derive(Clone)]
struct Server {
    clients: Arc<Mutex<HashMap<(usize, ChannelId), russh::server::Handle>>>,
    id: usize,
}

impl Server {
    async fn post(&mut self, data: CryptoVec) {
        let mut clients = self.clients.lock().await;
        for ((id, channel), ref mut s) in clients.iter_mut() {
            if *id != self.id {
                let _ = s.data(*channel, data.clone()).await;
            }
        }
    }
}

impl server::Server for Server {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
    }
    fn handle_session_error(&mut self, _error: <Self::Handler as russh::server::Handler>::Error) {
        eprintln!("Session error: {:#?}", _error);
    }
}

#[async_trait]
impl server::Handler for Server {
    type Error = russh::Error;

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        {
            let mut clients = self.clients.lock().await;
            clients.insert((self.id, channel.id()), session.handle());
        }
        info!("channel_open_session");
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        info!("exec_request(\"{}\")", String::from_utf8_lossy(data));
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        info!("pty request");
        //let _ = session.handle().data(channel, CryptoVec::from(format!("Not allowed\r\n"))).await;
        //session.data(channel, CryptoVec::from(format!("Not allowed\r\n")));
        let h = session.handle();
        let _ = h.data(channel, CryptoVec::from(format!("Not allowed\r\n"))).await;
        let _ = h.data(channel, CryptoVec::from(format!("Not allowed 2\r\n"))).await;
        //h.disconnect(Disconnect::ByApplication, "".into(), "".into()).await;
        //session.data(channel, CryptoVec::from(format!("Not allowed\r\n")));
        //session.data(channel, CryptoVec::from(format!("Not allowed2\r\n")));
        //session.eof(channel);
        h.exit_status_request(channel, 0).await;
        h.eof(channel).await;
        h.close(channel).await;
        Ok(())
    }

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        info!("auth_password: {}:{}", user, password);
        Ok(server::Auth::Accept)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Sending Ctrl+C ends the session and disconnects the client
        info!("data came");
        if data == [3] {
            return Err(russh::Error::Disconnect);
        }

        let data = CryptoVec::from(format!("Got data: {}\r\n", String::from_utf8_lossy(data)));
        self.post(data.clone()).await;
        session.data(channel, data);
        Ok(())
    }
}
