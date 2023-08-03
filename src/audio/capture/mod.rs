// SPDX-FileCopyrightText: Copyright 2023 tSVoI
// SPDX-License-Identifier: GPL-3.0-only

use bytes::{Bytes, BytesMut};
use flume::{Receiver, Sender};
use std::sync::{atomic::AtomicI8, Arc, Mutex};

use crate::audio::Audio;
use crate::audio::DeviceKind;
use miniaudio::{Device, DeviceConfig, DeviceType, Format, ShareMode};
use opus::{Application, Bitrate, Channels, Encoder};

pub struct AudioCapture {
    capture_device: Device,
    capture_tx: Sender<Bytes>,
    capture_rx: Receiver<Bytes>,
    intensity_tx: Sender<i8>,
    intensity_rx: Receiver<i8>,
    threshold: Arc<AtomicI8>,
    encoder: Arc<Mutex<Encoder>>,
}
impl AudioCapture {
    /// Creates a DeviceConfig for a capture device
    /// # Arguments
    /// * `device_name` - The name of the device to use
    /// * `channels` - The number of channels to use
    /// * `sample_rate` - The sample rate to use
    pub fn create_config(device_name: String, channels: u32, sample_rate: u32) -> DeviceConfig {
        let device_id = Audio::get_device_id(&device_name, DeviceKind::Capture);
        let mut config = DeviceConfig::new(DeviceType::Capture);
        config.capture_mut().set_format(Format::S16);
        config.capture_mut().set_channels(channels);
        config.capture_mut().set_share_mode(ShareMode::Shared);
        config.capture_mut().set_device_id(device_id);
        config.set_sample_rate(sample_rate);
        //config.set_period_size_in_milliseconds(10);
        config
    }

    /// Creates a new AudioCapture instance
    /// # Arguments
    /// * `device_id` - The DeviceId of the device to use
    /// * `channels` - The number of channels to use
    /// * `sample_rate` - The sample rate to use
    /// * `encoder_bitrate` - The bitrate to use for the encoder
    /// * `active_threshold` - The RMS threshold to record and encode the sample
    pub fn new(device_config: DeviceConfig, encoder_bitrate: i32, active_threshold: i8) -> Self {
        let (capture_tx, capture_rx) = flume::unbounded();
        let (intensity_tx, intensity_rx) = flume::unbounded();
        let capture_tx_clone = capture_tx.clone();
        let intensity_tx_clone = intensity_tx.clone();

        let threshold = Arc::new(AtomicI8::new(active_threshold));
        let threshold_clone = threshold.clone();

        let encoder_channels = match device_config.capture().channels() {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => panic!("Invalid channel count"),
        };
        let encoder = Arc::new(Mutex::new(
            Encoder::new(
                device_config.sample_rate(),
                encoder_channels,
                Application::Voip,
            )
            .unwrap(),
        ));
        encoder
            .lock()
            .unwrap()
            .set_bitrate(Bitrate::Bits(encoder_bitrate))
            .unwrap();
        encoder.lock().unwrap().set_vbr(true).unwrap();
        let encoder_clone = encoder.clone();

        let mut capture_device: Device = Device::new(None, &device_config).unwrap();
        capture_device.set_data_callback(move |_, _, input| {
            let input_samples = input.as_samples::<i16>();
            let num_samples = input_samples.len();

            //Calculate the sample RMS
            let sum: f32 = input_samples
                .iter()
                .map(|&s| (s as f32 / i16::MAX as f32).powi(2))
                .sum();
            let rms = (((sum / num_samples as f32).sqrt() + 0.0002) * 100.0) as i8;
            intensity_tx_clone.send(rms).unwrap();

            //If the RMS is above the threshold, encode and push to the queue
            if rms > threshold_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let encoded = Bytes::from(
                    encoder_clone
                        .lock()
                        .unwrap()
                        .encode_vec(input_samples, 512)
                        .unwrap(),
                );
                capture_tx_clone.send(encoded).unwrap();
            }
        });
        AudioCapture {
            capture_device,
            capture_tx,
            capture_rx,
            intensity_tx,
            intensity_rx,
            threshold,
            encoder,
        }
    }

    /// Starts the capture device
    pub fn start(&self) {
        self.capture_device.start().unwrap();
    }

    /// Stops the capture device
    pub fn stop(&self) {
        self.capture_device.stop().unwrap();
    }

    /// Returns the capture receiver
    pub fn get_capture_rx(&self) -> Receiver<Bytes> {
        self.capture_rx.clone()
    }

    /// Returns the intensity receiver
    pub fn get_intensity_rx(&self) -> Receiver<i8> {
        self.intensity_rx.clone()
    }

    /// Changes the threshold
    pub fn set_threshold(&self, value: i8) {
        self.threshold
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }

    /// Changes the encoder bitrate
    pub fn set_encoder_bitrate(&self, value: i32) {
        self.encoder
            .lock()
            .unwrap()
            .set_bitrate(Bitrate::Bits(value))
            .unwrap();
    }

    pub fn change_device(&mut self, device_name: String, channels: u32, sample_rate: u32) {
        self.stop();
        let config = Self::create_config(device_name, channels, sample_rate);

        let intensity_tx = self.intensity_tx.clone();
        let threshold = self.threshold.clone();
        let encoder_clone = self.encoder.clone();
        let capture_tx = self.capture_tx.clone();

        let mut capture_device = Device::new(None, &config).unwrap();
        capture_device.set_data_callback(move |_, _, input| {
            let input_samples = input.as_samples::<i16>();
            let num_samples = input_samples.len();

            //Calculate the sample RMS
            let sum: f32 = input_samples
                .iter()
                .map(|&s| (s as f32 / i16::MAX as f32).powi(2))
                .sum();
            let rms = (((sum / num_samples as f32).sqrt() + 0.0002) * 100.0) as i8;
            intensity_tx.send(rms).unwrap();

            //If the RMS is above the threshold, encode and push to the queue
            if rms > threshold.load(std::sync::atomic::Ordering::Relaxed) {
                let mut encoded = BytesMut::with_capacity(1024);
                let _ = encoder_clone
                    .lock()
                    .unwrap()
                    .encode(input_samples, encoded.as_mut())
                    .unwrap();
                capture_tx.send(encoded.freeze()).unwrap();
            }
        });
    }
}
