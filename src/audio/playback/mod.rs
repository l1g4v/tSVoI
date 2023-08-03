// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use bytes::Bytes;
use flume::{Receiver, Sender};
use std::{
    sync::{Arc, Mutex},
    thread,
};

use miniaudio::{Device, DeviceConfig, DeviceType, Format, ShareMode};
use opus::{Channels, Decoder};

use crate::audio::Audio;
use crate::audio::DeviceKind;
use crate::spawn_thread;

pub struct AudioPlayback {
    playback_device: Device,
    playback_tx: Sender<Bytes>,
    playback_rx: Receiver<Bytes>,
    playback_arc: Arc<Mutex<Vec<Bytes>>>,
}
impl AudioPlayback {
    /// Creates a DeviceConfig for a playback device
    /// # Arguments
    /// * `device_name` - The name of the device to use
    /// * `channels` - The number of channels to use
    /// * `sample_rate` - The sample rate to use
    pub fn create_config(device_name: &String, channels: u32, sample_rate: u32) -> DeviceConfig {
        let device_id = Audio::get_device_id(device_name, DeviceKind::Playback);
        let mut config = DeviceConfig::new(DeviceType::Playback);
        config.playback_mut().set_format(Format::S16);
        config.playback_mut().set_channels(channels);
        config.playback_mut().set_share_mode(ShareMode::Shared);
        config.playback_mut().set_device_id(device_id);
        config.set_sample_rate(sample_rate);
        //config.set_period_size_in_milliseconds(10);
        config
    }

    /// Creates a new AudioPlayback instance
    /// # Arguments
    /// * `config` - The DeviceConfig to use
    pub fn new(config: DeviceConfig) -> Self {
        let (playback_tx, playback_rx) = flume::unbounded::<Bytes>();
        let playback_arc = Arc::new(Mutex::new(Vec::<Bytes>::new()));
        let playback_clone = playback_arc.clone();

        let decoder_channels = match config.playback().channels() {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => panic!("Invalid channel count"),
        };
        let mut decoder = Decoder::new(config.sample_rate(), decoder_channels).unwrap();

        let mut playback_device: Device = Device::new(None, &config).unwrap();
        playback_device.set_data_callback(move |_, output, _| {
            let mut queue = playback_clone.lock().unwrap();
            let samples_len = output.as_samples_mut::<i16>().len();
            let decoded_buf = &mut [0; 1024];

            if queue.len() > 1 {
                //Decode opus packet
                let payload = queue.remove(0);
                let _ = decoder
                    .decode(&payload[..payload.len() - 1], decoded_buf, false)
                    .unwrap();
                let decoded = &mut decoded_buf[..samples_len];

                //Apply volume by scaling the decoded samples
                let volume = payload[payload.len() - 1] as f32 / 100.0;
                decoded
                    .iter_mut()
                    .for_each(|x| *x = (*x as f32 * volume) as i16);

                //Copy the decoded samples to the output buffer
                output.as_samples_mut::<i16>().copy_from_slice(&decoded);
            }
        });
        AudioPlayback {
            playback_device,
            playback_tx,
            playback_rx,
            playback_arc,
        }
    }

    /// Starts the playback device
    pub fn start(&self) {
        let err = self.playback_device.start();
        if let Err(e) = err {
            error!("Error starting playback device: {}", e);
        }
        let playback_arc = self.playback_arc.clone();
        let playback_rx = self.playback_rx.clone();

        spawn_thread!("playback device playstream listener", move || loop {
            if let Ok(payload) = playback_rx.recv() {
                if payload.len() == 1 {
                    if payload[0] == 0 {
                        break;
                    }
                }
                let mut queue = playback_arc.lock().unwrap();
                queue.push(payload);
            }
            thread::sleep(std::time::Duration::from_millis(1));
        });
    }
    pub fn stop(&self) {
        let _ = self.playback_tx.send(Bytes::from_static(&[0]));
        let _ = self.playback_device.stop();
    }

    /// Returns a clone of the playback queue
    pub fn get_playback_tx(&self) -> Sender<Bytes> {
        self.playback_tx.clone()
    }

    pub fn change_device(&mut self, device_name: &String, channels: u32, sample_rate: u32) {
        self.stop();
        let playback_clone = self.playback_arc.clone();
        let config = Self::create_config(device_name, channels, sample_rate);

        let decoder_channels = match config.playback().channels() {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => panic!("Invalid channel count"),
        };
        let mut decoder = Decoder::new(config.sample_rate(), decoder_channels).unwrap();

        let mut playback_device: Device = Device::new(None, &config).unwrap();
        playback_device.set_data_callback(move |_, output, _| {
            let mut queue = playback_clone.lock().unwrap();
            let samples_len = output.as_samples_mut::<i16>().len();
            let decoded_buf = &mut [0; 2048];

            if queue.len() > 1 {
                //Decode opus packet
                let payload = queue.remove(0);
                let _ = decoder
                    .decode(&payload[..payload.len() - 1], decoded_buf, false)
                    .unwrap();
                let decoded = &mut decoded_buf[..samples_len];

                //Apply volume by scaling the decoded samples
                let volume = payload[payload.len() - 1] as f32 / 100.0;
                decoded
                    .iter_mut()
                    .for_each(|x| *x = (*x as f32 * volume) as i16);

                //Copy the decoded samples to the output buffer
                output.as_samples_mut::<i16>().copy_from_slice(&decoded);
            }
        });
    }
}
