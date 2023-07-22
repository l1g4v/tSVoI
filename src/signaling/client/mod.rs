// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use log::{debug, error, info, warn};

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::aes::AES;
use crate::audio::playback::AudioPlayback;
use crate::audio::{Audio, playback};
use crate::audio_peer::{AudioPeer, self};
use crate::{signaling};

pub struct SignalingClient {
    id: u8,
    username: String,
    stream: TcpStream,
    cipher: Arc<AES>,
    //(username, address_candidate, peer)
    audio_peers: Arc<Mutex<HashMap<u8, (String, String, AudioPeer)>>>,
}
impl SignalingClient {
    pub fn new(username: String, address: String, key: String) -> Self {
        let cipher = Arc::new(AES::new(Some(key)));
        let audio_peers = Arc::new(Mutex::new(HashMap::new()));
        let try_stream = TcpStream::connect(address);
        if try_stream.is_err() {
            println!("Err: {:?}", try_stream.err());
            panic!("Failed to connect to server");
        } else {
            debug!("Connected to server");
        }
        let mut stream = try_stream.unwrap();
        //stream.write(&[1]);
        thread::sleep(std::time::Duration::from_millis(10));
        let buf = &mut [0; 1024];
        let try_recv = stream.read(buf);
        if try_recv.is_err() {
            panic!("Failed to read id");
        }
        let size = try_recv.unwrap();
        let encrypted = String::from_utf8_lossy(&buf[0..size]).to_string();
        debug!("Got encrypted {}", encrypted);
        let decrypted = cipher.decrypt(encrypted);
        let split = decrypted.split("¬").collect::<Vec<&str>>();
        let id = split[1].parse::<u8>().unwrap();
        debug!("Peer id is {}", id);
        SignalingClient {
            id,
            username,
            stream,
            cipher,
            audio_peers,
        }
    }
    pub fn run(&self, backend: String, playback_name: String) {
        let mut stream = self.stream.try_clone().unwrap();
        //Announce
        for i in 0..self.id {
            let address_candidate = signaling::get_address_ipv6();
            let announce = format!(
                "{}¬{}¬ann¬{}¬{}",
                i,
                self.id,
                self.username.clone(),
                address_candidate
            );
            let encrypted = self.cipher.encrypt(announce);
            debug!("Sending encrypted {}", encrypted);
            stream.write(encrypted.as_bytes()).unwrap();

            let mut peers = self.audio_peers.lock().unwrap();
            let audio_peer = AudioPeer::new(address_candidate);
            let item = ("".to_string(), "".to_string(), audio_peer);
            peers.insert(i, item);
        }
        thread::scope(move |scope| {
            let playback_mainclone = playback_name.clone();
            let buf = &mut [0; 2048];
            let audio_peers = self.audio_peers.clone();
            let bkc = backend.clone();
            loop {
                let backend = bkc.clone(); 
                let try_recv = stream.read(buf);
                if try_recv.is_err() {
                    error!("Failed to read from server, connection lost");
                    break;
                }
                let size = try_recv.unwrap();
                if size == 0 {
                    error!("Failed to read from server, connection lost");
                    break;
                }
                let encrypted = String::from_utf8_lossy(&buf[0..size]).to_string();
                debug!("Got encrypted {}", encrypted);
                let decrypted = self.cipher.decrypt(encrypted);
                debug!("Got decrypted {}", decrypted);
                let split = decrypted.split("¬").collect::<Vec<&str>>();
                let event = split[2];
                match event {
                    "ann" => {
                        let peer_id = split[1].parse::<u8>().unwrap();
                        let peer_username = split[3].to_string();
                        let peer_address_candidate = split[4].to_string();

                        let adress_candidate = signaling::get_address_ipv6();
                        let username = self.username.clone();

                        let mut peers = audio_peers.lock().unwrap();
                        let audio_peer = AudioPeer::new(adress_candidate.clone());
                        let item = (peer_username, peer_address_candidate, audio_peer);
                        peers.insert(peer_id, item);

                        let ack = format!(
                            "{}¬{}¬ack¬{}¬{}",
                            peer_id, self.id, username, adress_candidate
                        );
                        let encrypted = self.cipher.encrypt(ack);
                        debug!("Sending encrypted {}", encrypted);
                        stream.write(encrypted.as_bytes()).unwrap();
                    }
                    "ack" => {
                        let peer_id = split[1].parse::<u8>().unwrap();
                        let username = split[3];
                        let address_candidate = split[4];

                        let mut peers = audio_peers.lock().unwrap();
                        peers.get_mut(&peer_id).unwrap().0 = username.to_string();
                        peers.get_mut(&peer_id).unwrap().1 = address_candidate.to_string();

                        let ok = format!("{}¬{}¬ok", peer_id, self.id);
                        let encrypted = self.cipher.encrypt(ok);
                        debug!("Sending encrypted {}", encrypted);
                        stream.write(encrypted.as_bytes()).unwrap();
                    }
                    "ok" => {
                        let peer_id = split[1].parse::<u8>().unwrap();

                        let ok = format!("{}¬{}¬ko", peer_id, self.id);
                        let encrypted = self.cipher.encrypt(ok);
                        debug!("Sending encrypted {}", encrypted);
                        stream.write(encrypted.as_bytes()).unwrap();

                        let audio_peers_clone = audio_peers.clone();
                        let playback_clone = playback_mainclone.clone();
                        scope.spawn(move ||{
                            let peers = audio_peers_clone.lock().unwrap();
                            let item = peers.get(&peer_id);
                            let (_, address, peer) = item.unwrap();

                            let playback_id = Audio::get_device_id(backend.clone() ,&playback_clone, crate::audio::DeviceKind::Playback).unwrap();
                            let playback_config = AudioPlayback::create_config(playback_id, 2, 48_000);
                            peer.connect(address.clone(), backend.clone(), playback_config);
                            drop(peers);
                            loop{
                                thread::sleep(std::time::Duration::from_millis(1));
                            }
                            
                        }); 
                    }
                    "ko" => {
                        let peer_id = split[1].parse::<u8>().unwrap();

                        let audio_peers_clone = audio_peers.clone();
                        let playback_clone = playback_mainclone.clone();
                        scope.spawn(move ||{
                            let peers = audio_peers_clone.lock().unwrap();
                            let item = peers.get(&peer_id);
                            let (_, address, peer) = item.unwrap();

                            let playback_id = Audio::get_device_id(backend.clone(), &playback_clone, crate::audio::DeviceKind::Playback).unwrap();
                            let playback_config = AudioPlayback::create_config(playback_id, 2, 48_000);
                            peer.connect(address.clone(), backend.clone(), playback_config);
                            drop(peers);
                            loop{
                                thread::sleep(std::time::Duration::from_millis(1));
                            }
                            
                        });  
                    }
                    _ => {
                        error!("Unknown event {}", event);
                    }
                }
            }
        });
    }
    pub fn send_opus(&self, opus_packet: Vec<u8>) {
        let trylock = self.audio_peers.try_lock();
        if trylock.is_err(){
            error!("Failed to lock audio peers, err:{}", trylock.err().unwrap());
            return;
        }
        let peers = trylock.unwrap();
        debug!("N peers: {}", peers.len());
        for (_, _, peer) in peers.values() {
            debug!("Sending opus packet to peer");
            peer.send(opus_packet.clone());
        }
    }

    pub fn get_peers(&self) -> Vec<(u8, String)> {
        let peers = self.audio_peers.lock().unwrap();
        let mut result: Vec<(u8,String)> = Vec::new();
        peers.iter().for_each(|(id, (username, _, _))| {
            result.push((*id, username.clone()));
        });
        result
    }

    pub fn change_peer_volume(&self, peer_id: u8, volume: u8){
        let peers = self.audio_peers.lock().unwrap();
        let peer = peers.get(&peer_id);
        if peer.is_none(){
            error!("Peer {} not found", peer_id);
            return;
        }
        let (_, _, peer) = peer.unwrap();
        peer.change_volume(volume);
    }
}
