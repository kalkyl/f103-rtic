// $ cargo rb serial_circ_idle
// Receive serial data of arbitrary length using DMA
#![no_main]
#![no_std]

use f103_rtic as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f1xx_hal::pac, peripherals = true, dispatchers = [EXTI1])]
mod app {
    use heapless::Vec;
    use stm32f1xx_hal::{
        dma::{dma1::C5, CircBuffer, Event, Half, RxDma},
        pac::USART1,
        prelude::*,
        serial::{Config, Event::Idle, Rx, Serial},
    };
    const BUF_SIZE: usize = 8;

    #[shared]
    struct Shared {
        #[lock_free]
        recv: Option<CircBuffer<[u8; BUF_SIZE], RxDma<Rx<USART1>, C5>>>,
    }

    #[local]
    struct Local {}

    #[init(local = [rx_buf: [[u8; BUF_SIZE]; 2] = [[0; BUF_SIZE]; 2]])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let rcc = ctx.device.RCC.constrain();
        let mut flash = ctx.device.FLASH.constrain();
        let clocks = rcc.cfgr.freeze(&mut flash.acr);
        let mut afio = ctx.device.AFIO.constrain();
        let mut gpioa = ctx.device.GPIOA.split();
        let tx = gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh);
        let rx = gpioa.pa10;
        let mut serial = Serial::usart1(
            ctx.device.USART1,
            (tx, rx),
            &mut afio.mapr,
            Config::default().baudrate(115_200.bps()),
            clocks,
        );
        serial.listen(Idle);
        let mut channels = ctx.device.DMA1.split();
        channels.5.listen(Event::HalfTransfer);
        channels.5.listen(Event::TransferComplete);
        let (_, rx_serial) = serial.split();
        let rx = rx_serial.with_dma(channels.5);
        defmt::info!("Send me data of arbitrary length (<= 256 bytes)");
        (
            Shared {
                recv: Some(rx.circ_read(ctx.local.rx_buf)),
            },
            Local {},
            init::Monotonics(),
        )
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {}
    }

    // Triggers on RX half transfer or transfer completed
    #[task(binds = DMA1_CHANNEL5, shared = [recv], priority = 2)]
    fn on_rx(ctx: on_rx::Context) {
        let rx = ctx.shared.recv.as_mut().unwrap();
        let buf = rx.peek(|buf, _| *buf).unwrap();
        print::spawn(Vec::from_slice(&buf).unwrap()).ok();
    }

    // Triggers on serial line Idle
    #[task(binds = USART1, shared = [recv], priority = 2)]
    fn on_idle(ctx: on_idle::Context) {
        clear_idle_interrupt();
        let mut recv = ctx.shared.recv.take().unwrap();
        let readable_half = recv.readable_half().unwrap();
        let (buf, rx) = recv.stop();
        let pending = rx.channel.get_ndtr() as usize;
        let data = match readable_half {
            Half::First => &buf[1][..BUF_SIZE - pending],
            Half::Second => &buf[0][..2 * BUF_SIZE - pending],
        };
        print::spawn(Vec::from_slice(data).unwrap()).ok();
        ctx.shared.recv.replace(rx.circ_read(buf));
    }

    #[task(local = [msg: Vec<u8, 256> = Vec::new()], priority = 1)]
    fn print(ctx: print::Context, data: Vec<u8, BUF_SIZE>) {
        let is_completed = data.len() != BUF_SIZE;
        ctx.local.msg.extend(data);
        if is_completed {
            match core::str::from_utf8(ctx.local.msg.as_slice()) {
                Ok(str) => defmt::info!("{}", str),
                _ => defmt::info!("{:x}", ctx.local.msg.as_slice()),
            }
            ctx.local.msg.clear();
        }
    }

    #[inline]
    fn clear_idle_interrupt() {
        unsafe {
            let _ = (*USART1::ptr()).sr.read().idle();
            let _ = (*USART1::ptr()).dr.read().bits();
        }
    }
}
