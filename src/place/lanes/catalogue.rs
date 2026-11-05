// SPDX-License-Identifier: MPL-2.0
use super::*;
impl LiveLanes {
    pub(crate) fn publish_catalogue(&self, bytes: Vec<u8>) -> Result<(), String> {
        self._runtime
            .as_ref()
            .ok_or("place transport closed")?
            .block_on(self.catalogue.publish(bytes))
    }
    pub(crate) fn publish_catalogue_history(
        &self,
        moot: &gemot::moot::MootFile,
    ) -> Result<(), String> {
        self._runtime
            .as_ref()
            .ok_or("place transport closed")?
            .block_on(self.joined().moot.publish_catalogue_history(moot, 1024))
    }
}
pub(super) fn catalogue_topic(moot: [u8; 32]) -> [u8; 32] {
    // A separate public-metadata rendezvous for this random place identity.
    // This topic supplies no confidentiality or content authority.
    let mut bytes = b"turnstone/public-catalogue-topic/v1/".to_vec();
    bytes.extend(moot);
    *blake3::hash(&bytes).as_bytes()
}
