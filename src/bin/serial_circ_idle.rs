// $ cargo rb serial_circ_idle
#![no_main]
#![no_std]

use f103_rtic as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f1xx_hal::pac, peripherals = true, dispatchers = [EXTI1])]
mod app {
    use stm32f1xx_hal::{
        dma::{dma1::C5, CircBuffer, Event, Half, RxDma},
        pac::{DMA1, USART1},
        prelude::*,
        serial::{Config, Event::Idle, Rx, Serial},
    };
    const BUF_SIZE: usize = 8;

    #[shared]
    struct Shared {
        #[lock_free]
        recv: Option<CircBuffer<[u8; 8], RxDma<Rx<USART1>, C5>>>,
    }

    #[local]
    struct Local {}

    #[init(local = [rx_buf: [[u8; BUF_SIZE]; 2] = [[0; BUF_SIZE]; 2]])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        ctx.device.RCC.ahbenr.modify(|_, w| w.dma1en().enabled());
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
            Config::default().baudrate(9_600.bps()),
            clocks,
        );
        serial.listen(Idle);
        let mut channels = ctx.device.DMA1.split();
        channels.5.listen(Event::HalfTransfer);
        channels.5.listen(Event::TransferComplete);
        let (_, rx_serial) = serial.split();
        let rx = rx_serial.with_dma(channels.5);
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

    // Triggers on RX half transfer + transfer completed
    #[task(binds = DMA1_CHANNEL5, shared = [recv], priority = 2)]
    fn on_rx(ctx: on_rx::Context) {
        let (buf, half) = ctx
            .shared
            .recv
            .as_mut()
            .unwrap()
            .peek(|buf, half| (*buf, half))
            .unwrap();
        print::spawn(buf, half).ok();
    }

    // Triggers on serial line Idle
    #[task(binds = USART1, shared = [recv], priority = 2)]
    fn on_idle(ctx: on_idle::Context) {
        let mut recv = ctx.shared.recv.take().unwrap();
        let half = recv.readable_half().unwrap();
        let (buf, rx) = recv.stop();
        let ndtr = unsafe { &(*DMA1::ptr()) }.ch5.ndtr.read().bits() as usize;
        // Clear idle interrupt
        let _ = unsafe { (*USART1::ptr()).sr.read().idle() };
        let _ = unsafe { (*USART1::ptr()).dr.read().bits() };
        let data = match half {
            Half::First => &buf[1][..BUF_SIZE - ndtr],
            Half::Second => &buf[0][..BUF_SIZE - (ndtr - BUF_SIZE)],
        };
        defmt::info!("Idle {:x}", &data);
        ctx.shared.recv.replace(rx.circ_read(buf));
    }

    #[task(priority = 1, capacity = 4)]
    fn print(_: print::Context, data: [u8; BUF_SIZE], half: Half) {
        match half {
            Half::First => defmt::info!("First {:x} ", data),
            Half::Second => defmt::info!("Second {:x} ", data),
        }
    }
}