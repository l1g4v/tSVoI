#[macro_use]
extern crate log;
use std::env;
use std::thread;

use aead::generic_array::typenum::private::InternalMarker;
use base64::{engine::general_purpose, Engine as _};
use bytes::Buf;
use general_purpose::STANDARD_NO_PAD as BASE64;
// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

mod aes;
mod audio;
mod audio_peer;
mod signaling;

use aes::AES;
use audio::capture::AudioCapture;
use audio::playback::AudioPlayback;
use audio::Audio;
use audio::DeviceKind;
use audio_peer::AudioPeer;
use signaling::client;
use signaling::server::SignalingServer;
use signaling::client::SignalingClient;

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect::<Vec<String>>()[1..].to_vec();
    //dbg!(args);
    //args: <0 or 1 for server or client>
    //server: <username> <input device name> <output device name>
    //client: <username> <server address> <server key> <input device name> <output device name>
    let op = args[0].parse::<u8>().unwrap();
    match op{
        0 => {
            //Server
            let username = args[1].clone();
            let input_device_name = args[2].clone();
            let output_device_name = args[3].clone();
            let capture_device_config = AudioCapture::create_config(
                input_device_name,
                1,
                48_000,
            );
            let capture = AudioCapture::new(capture_device_config, 96_000, 0);
            let capture_rx = capture.get_capture_rx();
            capture.start();

            let server = SignalingServer::new(username);
            println!("{{ \"notification_code\": 0, \"server_address\": \"{}\", \"server_key\": \"{}\" }}", server.get_listen_address(), server.get_cipher_key());
            server.run(output_device_name);
            loop{
                if let Ok(data) = capture_rx.recv_timeout(std::time::Duration::from_millis(9)) {
                    server.send_opus(data);
                }
                thread::sleep(std::time::Duration::from_millis(1));
            }

        }
        1 => {
            //Client
            let username = args[1].clone();
            let server_address = args[2].clone();
            let server_key = args[3].clone();
            let input_device_name = args[4].clone();
            let output_device_name = args[5].clone();
            let capture_device_config = AudioCapture::create_config(
                input_device_name,
                1,
                48_000,
            );
            let capture = AudioCapture::new(capture_device_config, 96_000, 0);
            let capture_rx = capture.get_capture_rx();
            capture.start();

            let client = SignalingClient::new(username, &server_address, &server_key);
            client.run(output_device_name);
            loop{
                if let Ok(data) = capture_rx.recv_timeout(std::time::Duration::from_millis(9)) {
                    client.send_opus(data);
                }
                thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        3 => {
            Audio::print_devices();
        } 
        _ => {
            panic!("Invalid operation");
        }
    }
    


    /*let aes = AES::new(None).unwrap();
    let key = aes.get_key();

    let capconf = AudioCapture::create_config(
        "NoiseTorch Microphone for Built-in Audio Source".into(),
        1,
        48_000,
    );
    let capture = AudioCapture::new(capconf, 96_000, 0);
    let capture_rx = capture.get_capture_rx();
    capture.start();
    info!("Key {}", key);
    let peer1 = AudioPeer::new("[::1]:8080".into(), key.clone());
    let peer2 = AudioPeer::new("[::1]:8081".into(), key);

    peer1.connect("[::1]:8081".into(), "Built-in Audio Analog Stereo".into());
    peer2.connect("[::1]:8080".into(), "Built-in Audio Analog Stereo".into());

    //let placonf = AudioPlayback::create_config("Built-in Audio Analog Stereo".into(), 2, 48_000);
    //let playback = AudioPlayback::new(placonf);

    //let playback_tx = playback.get_playback_tx();

    //let placonf2 = AudioPlayback::create_config("Built-in Audio Analog Stereo".into(), 2, 48_000);
    //let playback2 = AudioPlayback::new(placonf2);

    //playback.start();

    loop {
        if let Ok(data) = capture_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            //playback_tx.send(data).unwrap();
            peer1.send(data);
        }
        thread::sleep(std::time::Duration::from_millis(1));
    }*/
}
