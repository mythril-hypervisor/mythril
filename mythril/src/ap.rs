/// A structure representing the data passed from the BSP to the AP
/// through the ap_startup logic.
#[repr(packed)]
pub struct ApData {
    /// This AP's index in the sequential list of all AP's
    pub idx: u64,
}
