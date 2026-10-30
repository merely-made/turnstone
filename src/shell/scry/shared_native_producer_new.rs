// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SharedNativeProducer {
    pub(super) fn new(producer: Rc<RefCell<PlatformWebSurfaceProducer>>) -> Self {
        Self(
            producer,
            Rc::new(RefCell::new(KeyboardDispatchQueue::default())),
        )
    }

    pub(super) fn drive_keyboard(&self) -> Result<(), native::WebSurfaceError> {
        let Some(params) = self.1.borrow_mut().next(Instant::now())? else {
            return Ok(());
        };
        let weak_queue = Rc::downgrade(&self.1);
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |result: windows::core::Result<()>, json_result: String| {
                if let Some(queue) = weak_queue.upgrade() {
                    queue.borrow_mut().complete(keyboard_completion(
                        result.map_err(|error| error.to_string()),
                        &json_result,
                    ));
                }
                Ok(())
            },
        ));
        let method = CoTaskMemPWSTR::from("Input.dispatchKeyEvent");
        let params = CoTaskMemPWSTR::from(params.as_str());
        // SAFETY: the producer and callback stay on their construction thread;
        // WebView2 retains the callback for this asynchronous operation. The
        // callback holds only Weak queue state and cannot retain a closed page.
        let result = unsafe {
            tracing::debug!(target: "turnstone::scry_input", "begin keyboard CDP submission");
            self.0.borrow().webview().CallDevToolsProtocolMethod(
                *method.as_ref().as_pcwstr(),
                *params.as_ref().as_pcwstr(),
                &handler,
            )
        };
        tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished keyboard CDP submission");
        if let Err(error) = result {
            self.1
                .borrow_mut()
                .complete(Err(format!("Scry keyboard CDP dispatch failed: {error}")));
        }
        self.1.borrow().check_failure()
    }
}
