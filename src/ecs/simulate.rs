use embassy_executor::task;
use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use esp_hal::twai::{EspTwaiFrame, Id, StandardId};
use embassy_futures::select::Either;

use crate::{CanFrameReceiver, CanFrameSender};

#[task]
pub(crate) async fn simulate_if_no_traffic(receiver: CanFrameReceiver<'static>, sender: CanFrameSender<'static>) {
    match select(receiver.receive(), Timer::after(Duration::from_secs(10))).await {
        Either::First(_) => return,
        Either::Second(_) => {},
    }
    let mut count = 0_u8;
    loop {
        let frame = EspTwaiFrame::new(Id::Standard(StandardId::new(0x208).unwrap()), &[count, count, count, count, 0, 0, 0, 0]).unwrap();
        sender.send(frame).await;
        Timer::after(Duration::from_millis(50)).await;
        count+=1;
        if count > 200 {
             count = 0 
        }
        
    }

}
