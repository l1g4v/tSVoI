#[macro_use]
extern crate log;
// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only
use serde_json::Value;
use std::env;
use std::thread;

mod aes;
mod audio;
mod audio_peer;
mod signaling;
use audio::capture::AudioCapture;
use audio::Audio;
use signaling::client::SignalingClient;
use signaling::server::SignalingServer;

#[macro_export]
macro_rules! spawn_thread {
    ($name:expr, $body:expr) => {
        thread::Builder::new()
            .name($name.to_string())
            .spawn($body)
            .unwrap();
    };
}

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect::<Vec<String>>()[1..].to_vec();
    //stdin handler
    let (stdin_tx, stdin_rx) = flume::bounded::<(u8, u8, u8, u16, u16, Option<String>)>(1);
    spawn_thread!("stdin thread" ,move || {
        loop {
            //format { "op_code": n, ... }
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let try_parse = serde_json::from_str::<Value>(&input);
            if try_parse.is_err() {
                println!("{{ \"event_code\": -1, \"error\": \"Failed to parse stdin\" }}");
                continue;
            }
            let parsed = try_parse.unwrap();
            let op_code = parsed["op_code"].as_u64().unwrap() as u8;
            match op_code {
                0 | 1 => {
                    let device = parsed["device"].as_str().unwrap().to_string();
                    let channels = parsed["channels"].as_u64().unwrap() as u8;
                    let sample_rate = parsed["sample_rate"].as_u64().unwrap() as u16;
                    let _ = stdin_tx.send((op_code, channels, 0, sample_rate, 0, Some(device)));
                }
                2 => {
                    let peer_id = parsed["peer_id"].as_u64().unwrap() as u8;
                    let volume = parsed["volume"].as_u64().unwrap() as u8;
                    let _ = stdin_tx.send((op_code, peer_id, volume, 0, 0, None));
                }
                3 => {
                    let bitrate = parsed["bitrate"].as_u64().unwrap() as u16;
                    let _ = stdin_tx.send((op_code, 0, 0, bitrate, 0, None));
                }

                _ => {}
            }
        }
    });

    //args: <0 or 1 for server or client>
    //server: <username> <input device name> <output device name>
    //client: <username> <server address> <server key> <input device name> <output device name>
    let op = args[0].parse::<u8>().unwrap();
    match op {
        0 => {
            //Server
            let username = args[1].clone();
            let input_device_name = args[2].clone();
            let output_device_name = args[3].clone();
            let capture_device_config = AudioCapture::create_config(input_device_name, 1, 48_000);
            let mut capture = AudioCapture::new(capture_device_config, 64_000, 0);
            let capture_rx = capture.get_capture_rx();
            capture.start();

            let server = SignalingServer::new(username);
            println!(
                "{{ \"event_code\": 0, \"server_address\": \"{}\", \"server_key\": \"{}\" }}",
                server.get_listen_address(),
                server.get_cipher_key()
            );
            server.run(output_device_name);
            loop {
                if let Ok(data) = stdin_rx.try_recv() {
                    match data.0 {
                        0 => {
                            capture.change_device(data.5.unwrap(), data.1 as u32, data.3 as u32);
                        }
                        1 => {
                            server.change_playback(&data.5.unwrap(), data.1 as u32, data.2 as u32);
                        }
                        2 => {
                            server.change_peer_volume(data.1, data.2);
                        }
                        3 => {
                            capture.set_encoder_bitrate(data.3 as i32);
                        }
                        _ => {}
                    }
                }
                if let Ok(data) = capture_rx.recv() {
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
            let capture_device_config = AudioCapture::create_config(input_device_name, 1, 48_000);
            let mut capture = AudioCapture::new(capture_device_config, 64_000, 0);
            let capture_rx = capture.get_capture_rx();
            capture.start();

            let client = SignalingClient::new(username, &server_address, &server_key);
            client.run(output_device_name);
            loop {
                if let Ok(data) = stdin_rx.try_recv() {
                    match data.0 {
                        0 => {
                            capture.change_device(data.5.unwrap(), data.1 as u32, data.3 as u32);
                        }
                        1 => {
                            client.change_playback(&data.5.unwrap(), data.1 as u32, data.2 as u32);
                        }
                        2 => {
                            client.change_peer_volume(data.1, data.2);
                        }
                        3 => {
                            capture.set_encoder_bitrate(data.3 as i32);
                        }
                        _ => {}
                    }
                }
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
}
