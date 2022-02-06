// $ DEFMT_LOG=info cargo rb rtic-tick
#![no_main]
#![no_std]

use f103_rtic as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f1xx_hal::pac, dispatchers = [USART1])]
mod app {
    use f103_rtic::mono::{ExtU32, MonoTimer};
    use stm32f1xx_hal::{pac, prelude::*};

    #[shared]
    struct Shared {}

    #[local]
    struct Local {}

    #[monotonic(binds = TIM2, default = true)]
    type Monotonic = MonoTimer<pac::TIM2, 1_000_000>;

    #[init]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let rcc = ctx.device.RCC.constrain();
        let mut flash = ctx.device.FLASH.constrain();
        let clocks = rcc.cfgr.sysclk(48.mhz()).freeze(&mut flash.acr);
        let mono = Monotonic::new(ctx.device.TIM2, &clocks);
        tick::spawn().ok();
        (Shared {}, Local {}, init::Monotonics(mono))
    }

    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            continue;
        }
    }

    #[task]
    fn tick(_: tick::Context) {
        defmt::info!("Tick!");
        tick::spawn_after(1.secs()).ok();
    }
}
