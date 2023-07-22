#[macro_use]
extern crate log;
use std::thread;

use base64::{Engine as _, engine::general_purpose};
use bytes::Buf;
use general_purpose::STANDARD_NO_PAD as BASE64;
// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only 


mod aes;
mod audio;
mod audio_peer;
use audio::Audio;
use audio::DeviceKind;
use audio::capture::AudioCapture;
use audio::playback::AudioPlayback;
use audio_peer::AudioPeer;
use aes::AES;

fn main() {
    env_logger::init();
    Audio::print_devices();

    let aes = AES::new(None).unwrap();
    let key = aes.get_key();

    let capconf = AudioCapture::create_config("NoiseTorch Microphone for Built-in Audio Source".into(), 1, 48_000);
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
    }
}
