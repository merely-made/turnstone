// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::ShellEvent;
use std::sync::mpsc::{Receiver, SendError, Sender, channel};

#[derive(Clone)]
pub struct ShellEventProxy {
    wake: winit::event_loop::EventLoopProxy,
    sender: Sender<ShellEvent>,
}

impl ShellEventProxy {
    pub fn channel(wake: winit::event_loop::EventLoopProxy) -> (Self, Receiver<ShellEvent>) {
        let (sender, receiver) = channel();
        (Self { wake, sender }, receiver)
    }

    pub fn send_event(&self, event: ShellEvent) -> Result<(), SendError<ShellEvent>> {
        self.sender.send(event)?;
        self.wake.wake_up();
        Ok(())
    }
}
