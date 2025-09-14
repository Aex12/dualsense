use std::{collections::HashMap, sync::Arc};

use async_hid::{DeviceEvent, DeviceId, HidBackend, HidError, HidResult};
use futures_lite::StreamExt;
use smol::lock::Mutex;

use crate::dualsense::async_hid::DualSense;

#[derive(Debug)]
pub enum DeviceManagerEvent {
    Connected(DeviceId, String),
    Disconnected(DeviceId),
    BatteryUpdate(DeviceId, (u8, bool)), // percentage, charging
}

pub struct DeviceManager {
    hid: HidBackend,
    opened_devices: Mutex<HashMap<DeviceId, Arc<DualSense>>>,
    event_handler: Option<Arc<Box<dyn Fn(DeviceManagerEvent) + Send + Sync + 'static>>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            hid: HidBackend::default(),
            opened_devices: Mutex::new(HashMap::new()),
            event_handler: None,
        }
    }

    pub fn set_event_handler<F>(&mut self, handler: F)
    where
        F: Fn(DeviceManagerEvent) + Send + Sync + 'static,
    {
        self.event_handler = Some(Arc::new(Box::new(handler)));
    }

    async fn insert_device(&self, device: DualSense) {
        let device_id = device.device_id().clone();
        let device_name = device.name();

        let device = Arc::new(device);
        self.opened_devices
            .lock()
            .await
            .insert(device_id.clone(), device.clone());

        if let Some(handler) = &self.event_handler {
            handler(DeviceManagerEvent::Connected(
                device_id.clone(),
                device_name,
            ));
            self.update_device_status(device_id, device).await;
        }
    }

    async fn close_device(&self, device_id: &DeviceId) {
        self.opened_devices.lock().await.remove(device_id);

        if let Some(handler) = &self.event_handler {
            handler(DeviceManagerEvent::Disconnected(device_id.clone()));
        }
    }

    async fn open_device_id(&self, device_id: DeviceId) -> HidResult<()> {
        if self.opened_devices.lock().await.get(&device_id).is_some() {
            return Ok(());
        }
        let device = DualSense::open_device_id(&self.hid, &device_id).await?;
        self.insert_device(device).await;
        Ok(())
    }

    pub async fn open_all_devices(&self) -> HidResult<()> {
        let devices = DualSense::enumerate(&self.hid)
            .await?
            .map(|device| smol::spawn(async move { DualSense::open_device(device).await }))
            .collect::<Vec<_>>()
            .await;

        for device in devices {
            if let Ok(device) = device.await {
                self.insert_device(device).await;
            }
        }

        Ok(())
    }

    pub async fn watch_pnp(&self) -> HidResult<()> {
        let mut watch_stream = self.hid.watch()?;
        while let Some(event) = watch_stream.next().await {
            match event {
                DeviceEvent::Connected(device_id) => {
                    let _ = self.open_device_id(device_id).await;
                }
                DeviceEvent::Disconnected(device_id) => {
                    self.close_device(&device_id).await;
                }
            }
        }
        Ok(())
    }

    pub async fn update_device_status(&self, device_id: DeviceId, device: Arc<DualSense>) -> () {
        if self.event_handler.is_none() {
            return;
        }
        let event_handler = self.event_handler.as_ref().unwrap().clone();
        let _ = smol::spawn(async move {
            let mut ds_conn = device.connect().await?;

            let report = ds_conn.read_input_report().await?;
            let (capacity, charging) = report.battery();

            event_handler(DeviceManagerEvent::BatteryUpdate(
                device_id,
                (capacity, charging),
            ));

            Ok::<(), HidError>(())
        })
        .await;
    }

    pub async fn update_status(&self) -> () {
        if self.event_handler.is_none() {
            return;
        }
        // clone the hashmap to avoid holding the lock while emitting events
        let devices = self.opened_devices.lock().await.clone();
        let tasks = devices.iter().map(|(device_id, device)| {
            self.update_device_status(device_id.clone(), device.clone())
        });

        for task in tasks {
            let _ = task.await;
        }
    }
}
