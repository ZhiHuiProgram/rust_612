use libc::{sigaction, ucontext_t, SA_SIGINFO};
use std::mem;
use std::ptr;

pub const FW_VERSION: &str = "A612LV-1-V1_0_0-251020";

extern "C" fn crash(sig: i32, _: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let uc = ctx as *mut ucontext_t;
        
        #[cfg(target_arch = "arm")]
        let (pc, lr) = (
            (*uc).uc_mcontext.arm_pc,
            (*uc).uc_mcontext.arm_lr
        );
        
        let msg = format!("CRASH sig={} PC={:#x} LR={:#x}\n", sig, pc, lr);
        libc::write(2, msg.as_ptr() as *const _, msg.len());
        
        std::fs::write("/var/log/crash.log", &msg).ok();
        libc::sync();
        // libc::reboot(libc::RB_AUTOBOOT);
        libc::_exit(1);
    }
}
pub fn setup_crash_handler() {
    unsafe {
        let mut sa: sigaction = mem::zeroed();
        sa.sa_sigaction = crash as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = SA_SIGINFO;

        libc::sigaction(libc::SIGSEGV, &sa, ptr::null_mut());
        libc::sigaction(libc::SIGBUS, &sa, ptr::null_mut());
        libc::sigaction(libc::SIGFPE, &sa, ptr::null_mut());
        libc::sigaction(libc::SIGILL, &sa, ptr::null_mut());
    }
}