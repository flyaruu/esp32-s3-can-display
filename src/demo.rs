pub fn thing() {

    let can_tx = peripherals.GPIO33; 
    let can_rx = peripherals.GPIO21; 


    let mut can = TwaiConfiguration::new(
        peripherals.TWAI0,
        can_rx,
        can_tx,
        BaudRate::B500K,
        TwaiMode::Normal,
    )
    .into_async();

    let can = can.start();

    
}


pub struct EspTwaiFrame {
    id: Id,
    dlc: usize,
    data: [u8; 8],
}

#[task]
async fn process_frame(mut twai: Twai<'static, Async>) {
    loop {
        match twai.receive_async().await {
            Ok(message) => {
                // process message
            }
            Err(e) => {
                warn!("Error reading message: {e:?}");
            }
        }
    }
}
