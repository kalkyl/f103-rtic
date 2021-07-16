// $ cargo rb serial
#![no_main]
#![no_std]

use f103_rtic as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f1xx_hal::pac, peripherals = true, dispatchers = [EXTI1])]
mod app {
    use stm32f1xx_hal::{
        dma::{
            dma1::{C4, C5},
            Event, RxDma, Transfer, TxDma, R, W,
        },
        pac::USART1,
        prelude::*,
        serial::{Config, Rx, Serial, Tx},
    };
    const BUF_SIZE: usize = 8;

    pub enum TxTransfer {
        Running(Transfer<R, &'static mut [u8; BUF_SIZE], TxDma<Tx<USART1>, C4>>),
        Idle(&'static mut [u8; BUF_SIZE], TxDma<Tx<USART1>, C4>),
    }

    #[shared]
    struct Shared {
        #[lock_free]
        send: Option<TxTransfer>,
    }

    #[local]
    struct Local {
        recv: Option<Transfer<W, &'static mut [u8; BUF_SIZE], RxDma<Rx<USART1>, C5>>>,
    }

    #[init(local = [tx_buf: [u8; BUF_SIZE] = [0; BUF_SIZE], rx_buf: [u8; BUF_SIZE] = [0; BUF_SIZE]])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        ctx.device.RCC.ahbenr.modify(|_, w| w.dma1en().enabled());
        let mut rcc = ctx.device.RCC.constrain();
        let mut flash = ctx.device.FLASH.constrain();
        let clocks = rcc.cfgr.freeze(&mut flash.acr);
        let mut afio = ctx.device.AFIO.constrain(&mut rcc.apb2);
        let mut gpioa = ctx.device.GPIOA.split(&mut rcc.apb2);
        let tx = gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh);
        let rx = gpioa.pa10;
        let serial = Serial::usart1(
            ctx.device.USART1,
            (tx, rx),
            &mut afio.mapr,
            Config::default().baudrate(9_600.bps()),
            clocks,
            &mut rcc.apb2,
        );
        let mut channels = ctx.device.DMA1.split(&mut rcc.ahb);
        channels.4.listen(Event::TransferComplete);
        channels.5.listen(Event::TransferComplete);
        let (tx_serial, rx_serial) = serial.split();
        let tx = tx_serial.with_dma(channels.4);
        let rx = rx_serial.with_dma(channels.5);
        (
            Shared {
                send: Some(TxTransfer::Idle(ctx.local.tx_buf, tx)),
            },
            Local {
                recv: Some(rx.read(ctx.local.rx_buf)),
            },
            init::Monotonics(),
        )
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {}
    }

    // Triggers on RX transfer completed
    #[task(binds = DMA1_CHANNEL5, local = [recv], priority = 2)]
    fn on_rx(ctx: on_rx::Context) {
        let (rx_buf, rx) = ctx.local.recv.take().unwrap().wait();
        echo::spawn(*rx_buf).ok();
        ctx.local.recv.replace(rx.read(rx_buf));
    }

    #[task(shared = [send], priority = 1, capacity = 4)]
    fn echo(ctx: echo::Context, data: [u8; BUF_SIZE]) {
        defmt::info!("Received {:?}", data);
        let send = ctx.shared.send;
        let (tx_buf, tx) = match send.take().unwrap() {
            TxTransfer::Idle(buf, tx) => (buf, tx),
            TxTransfer::Running(transfer) => transfer.wait(),
        };
        tx_buf.copy_from_slice(&data[..]);
        send.replace(TxTransfer::Running(tx.write(tx_buf)));
    }

    // Triggers on TX transfer completed
    #[task(binds = DMA1_CHANNEL4, shared = [send], priority = 1)]
    fn on_tx(ctx: on_tx::Context) {
        let send = ctx.shared.send;
        let (tx_buf, tx) = match send.take().unwrap() {
            TxTransfer::Idle(buf, tx) => (buf, tx),
            TxTransfer::Running(transfer) => transfer.wait(),
        };
        defmt::info!("Sent {:?}", tx_buf);
        send.replace(TxTransfer::Idle(tx_buf, tx));
    }
}
