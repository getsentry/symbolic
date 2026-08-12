#![no_main]
#![no_std]

use core::hint::black_box;
use core::panic::PanicInfo;

#[link(name = "System")]
unsafe extern "C" {}

static FUNCTIONS: [fn(u64) -> u64; 48] = [
    add::<0>, add::<1>, add::<2>, add::<3>, add::<4>, add::<5>, add::<6>, add::<7>,
    add::<8>, add::<9>, add::<10>, add::<11>, add::<12>, add::<13>, add::<14>, add::<15>,
    add::<16>, add::<17>, add::<18>, add::<19>, add::<20>, add::<21>, add::<22>, add::<23>,
    add::<24>, add::<25>, add::<26>, add::<27>, add::<28>, add::<29>, add::<30>, add::<31>,
    add::<32>, add::<33>, add::<34>, add::<35>, add::<36>, add::<37>, add::<38>, add::<39>,
    add::<40>, add::<41>, add::<42>, add::<43>, add::<44>, add::<45>, add::<46>, add::<47>,
];

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {}
}

#[inline(never)]
fn add<const VALUE: u64>(input: u64) -> u64 {
    input + VALUE
}

#[unsafe(no_mangle)]
extern "C" fn main() -> i32 {
    FUNCTIONS
        .iter()
        .fold(0, |value, function| function(black_box(value))) as i32
}
