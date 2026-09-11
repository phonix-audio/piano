//! The sample split across cores.
//!
//! One sample of this instrument is two pieces of work, in this order: every
//! sounding string reads the plate and pushes back on it, then the plate takes
//! every push and steps. Both are large — at eighty voices the first is around
//! six hundred thousand mode-operations and the second is eleven thousand — and
//! both divide cleanly, which is the whole reason this module can exist:
//!
//!   * **The voices do not interact within a sample.** A voice reads `q1` and
//!     writes into `drive`, and `drive` is not consumed until the plate ticks.
//!     No voice has ever seen another's push in the same sample, whatever order
//!     the loop ran in, so splitting the loop across threads changes nothing
//!     about the physics. Each worker accumulates into its OWN drive buffer, so
//!     there is no shared write at all.
//!   * **The plate's modes do not interact.** A modal bank is N independent
//!     two-pole recursions; a range of them can be stepped without knowing
//!     anything about the others. Only the readouts are sums, and a sum splits
//!     into partial sums.
//!
//! What is NOT divisible is the boundary between the two: every push has to
//! land before the plate steps. So there are two barriers per sample, and they
//! are spin barriers — at 48 kHz a sample is twenty microseconds, and a
//! futex round trip would eat it. Workers spin only inside a block; between
//! blocks they park on a condition variable, so an idle plugin costs nothing.
//!
//! Determinism: the work is split by fixed index ranges, so a given voice
//! always lands on the same worker for a given active set, and partial sums are
//! combined in worker order. Two runs of the same input give the same bits.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// How the calling thread is scheduled, so the workers can be scheduled the
/// same way.
///
/// This matters more than anything else in this file. The workers are joined to
/// the audio callback by a barrier on EVERY sample: at a 1024-frame block that
/// is two thousand barriers per callback, and if one worker is an ordinary
/// thread that the desktop can preempt, all of them wait out a scheduler
/// quantum — milliseconds, against a budget of twenty microseconds. Measured in
/// the host before this was added: a passage that costs 20 ms of work spiking
/// to 57 ms, with xruns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheduling {
    #[cfg(target_os = "linux")]
    policy: i32,
    #[cfg(target_os = "linux")]
    priority: i32,
}

impl Scheduling {
    /// Read the calling thread's policy and priority. The pool is built from
    /// inside the audio callback, so this is the audio thread's own.
    pub fn of_current_thread() -> Self {
        #[cfg(target_os = "linux")]
        unsafe {
            let mut param: libc::sched_param = std::mem::zeroed();
            let mut policy: libc::c_int = 0;
            if libc::pthread_getschedparam(libc::pthread_self(), &mut policy, &mut param) == 0 {
                return Self { policy, priority: param.sched_priority };
            }
            Self { policy: libc::SCHED_OTHER, priority: 0 }
        }
        #[cfg(not(target_os = "linux"))]
        Self {}
    }

    /// Put the calling thread on the same footing, one priority step below the
    /// thread this was read from — below, so a worker can never keep the audio
    /// callback itself off a core.
    ///
    /// Failure is not an error: without `RLIMIT_RTPRIO` the request is refused
    /// and the pool simply runs at ordinary priority, which is what it did
    /// before.
    pub fn apply_to_current_thread(self) {
        #[cfg(target_os = "linux")]
        unsafe {
            if self.policy != libc::SCHED_FIFO && self.policy != libc::SCHED_RR {
                return;
            }
            let mut param: libc::sched_param = std::mem::zeroed();
            param.sched_priority = (self.priority - 1).max(1);
            let _ = libc::pthread_setschedparam(libc::pthread_self(), self.policy, &param);
        }
    }

    /// Whether the thread this was read from is real-time at all.
    pub fn is_realtime(self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.policy == libc::SCHED_FIFO || self.policy == libc::SCHED_RR
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}

/// The fewest voices a participant must be given for its share to be worth a
/// barrier.
///
/// The barriers are per SAMPLE, so what matters is not how many voices there
/// are but how much work each participant gets between two of them. Split eight
/// voices nine ways and every thread does one voice and then waits — measured,
/// that was SLOWER than one thread doing all eight, and on a busy machine far
/// slower. The participant count therefore follows the polyphony.
///
/// **Six until 2026-08-23, and six left the machine idle.** Six is a MINIMUM
/// grain, but `participants_for` uses it as a divisor, so at thirty voices it
/// asked for five participants where nine were available: four cores watched.
/// Swept on the glissando at 1024 frames — warm-up thrown away first, then every
/// setting measured three times round-robin, medians:
///
/// ```text
/// voices/participant   1    2    3    4    6   10
/// overruns of 187     26   21   24   83   98  107
/// ```
///
/// The cliff between three and four is exactly where the pool stops saturating:
/// 30/3 = 10 participants (capped to nine, full machine), 30/4 = 7 (two cores
/// idle). Three is the largest grain that still fills this machine, so it keeps
/// the most work between barriers while leaving none of it undone.
///
/// The sweep is worth a caution. Run ONE WAY on a cold machine it printed 148,
/// 183, 187, 129, 117, 104 and run back it printed 108, 103, 50, 10, 21, 49 —
/// a trend that follows the rank of the trial, not the setting, from both ends.
/// A single pass over this would have "measured" whatever ran last.
pub const VOICES_PER_PARTICIPANT: usize = 3;

/// Overridable for the bench that tunes it.
pub static VPP: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(VOICES_PER_PARTICIPANT);

/// Below this many sounding voices nothing is shared at all.
pub const PARALLEL_FROM: usize = 2 * VOICES_PER_PARTICIPANT;

/// How many participants — the audio thread included — should share a sample of
/// this many voices, given `workers` are available.
pub fn participants_for(live: usize, workers: usize) -> usize {
    let vpp = VPP.load(std::sync::atomic::Ordering::Relaxed).max(1);
    if live < 2 * vpp {
        return 1;
    }
    (live / vpp).clamp(1, workers + 1)
}

/// How many workers to run by default: the cores, less two — one for the audio
/// callback itself and one for whatever else the machine is doing.
pub fn default_workers() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    cores.saturating_sub(2).min(8)
}

/// A barrier that spins. `wait` returns once every participant has arrived.
///
/// Sense-reversing, so it can be reused sample after sample without a reset:
/// each pass flips the sense flag, and a thread waits for the flag to differ
/// from the sense it arrived with.
pub struct SpinBarrier {
    count: AtomicUsize,
    sense: AtomicBool,
    total: AtomicUsize,
}

impl SpinBarrier {
    pub fn new(total: usize) -> Self {
        Self {
            count: AtomicUsize::new(0),
            sense: AtomicBool::new(false),
            total: AtomicUsize::new(total.max(1)),
        }
    }

    /// Change how many threads the barrier expects. Only safe between blocks,
    /// with every worker parked.
    pub fn set_total(&self, total: usize) {
        self.total.store(total.max(1), Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
    }

    /// Arrive, and leave when everyone has.
    ///
    /// `Release` on arrival and `Acquire` on departure are what publish the
    /// drive buffers written before the barrier to the threads that read them
    /// after it.
    pub fn wait(&self) {
        let my_sense = self.sense.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        if self.count.fetch_add(1, Ordering::AcqRel) + 1 == total {
            self.count.store(0, Ordering::Relaxed);
            self.sense.store(!my_sense, Ordering::Release);
            return;
        }
        let mut spins = 0u32;
        while self.sense.load(Ordering::Acquire) == my_sense {
            spins += 1;
            // A few hundred spins is longer than any sample's worth of work;
            // past that the machine is oversubscribed and yielding is kinder
            // than burning the core.
            if spins < 512 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
    }
}

/// The work of one block, published by the audio thread and read by every
/// worker until the block ends.
///
/// Raw pointers, and the safety argument is structural rather than local: the
/// audio thread publishes a job, then does nothing outside the same barrier
/// pair the workers are in, so nobody touches these objects except inside the
/// phase that owns them. Within a phase the participants' writes are disjoint —
/// each owns a range of voices, then a range of the plate's modes.
#[derive(Clone, Copy)]
struct Job {
    voices: *mut crate::voice::Voice,
    board: *const crate::soundboard::Soundboard,
    /// Where each participant leaves the force its voices produced.
    forces: *mut f64,
    /// The sounding voices' indices, and their weight arrays, gathered once per
    /// block by the audio thread.
    idx: *const usize,
    shapes: *const std::sync::Arc<Vec<f64>>,
    n_live: usize,
    /// Each participant's partial ear velocities, combined by the audio thread.
    ears: *mut (f64, f64),
    /// Per part, the listening points over the localised modes, when the plate
    /// is read split (`kind` 0); the pair above is then the global modes.
    ears_high: *mut (f64, f64),
    /// Per part, the sum of what its voices' microphones heard this frame.
    heard: *mut (f64, f64),
    /// Relative throughputs, and where each participant reports the time its
    /// share of the strings took this block.
    weights: *const f64,
    spent: *mut f64,
    frames: usize,
    parts: usize,
    /// 0 = per-sample coupling (the exact model), 1 = block-major decoupled:
    /// each participant rolls its voice share's forces for the whole block,
    /// one barrier, then advances its mode range for the whole block. Two
    /// barriers per BLOCK against two per sample.
    kind: u8,
    /// Block mode only: per-participant ear rows, `parts * frames`, unscaled.
    bears: *mut (f64, f64),
    /// Block mode only: per-participant tile scratch, `parts * (BLOCK_TILE * frames)`.
    scratch: *mut f64,
    /// Block mode only: the single-precision force matrix and tile scratch.
    forces32: *mut f32,
    scratch32: *mut f32,
    /// Block mode only: the voices' single-precision attachment weights.
    shapes_f32: *const std::sync::Arc<Vec<f32>>,
}

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    const EMPTY: Job = Job {
        voices: std::ptr::null_mut(),
        board: std::ptr::null(),
        forces: std::ptr::null_mut(),
        idx: std::ptr::null(),
        shapes: std::ptr::null(),
        n_live: 0,
        ears: std::ptr::null_mut(),
        ears_high: std::ptr::null_mut(),
        heard: std::ptr::null_mut(),
        weights: std::ptr::null(),
        spent: std::ptr::null_mut(),
        frames: 0,
        parts: 1,
        kind: 0,
        bears: std::ptr::null_mut(),
        scratch: std::ptr::null_mut(),
        forces32: std::ptr::null_mut(),
        scratch32: std::ptr::null_mut(),
        shapes_f32: std::ptr::null(),
    };
}

struct Control {
    /// Bumped when a block starts; workers park until it changes.
    epoch: AtomicUsize,
    stop: AtomicBool,
    job: Mutex<Job>,
    lock: Mutex<usize>,
    cv: Condvar,
}

/// The worker pool: threads that live as long as the engine and take a share of
/// every sample of every block.
pub struct VoicePool {
    control: Arc<Control>,
    barrier: Arc<SpinBarrier>,
    handles: Vec<std::thread::JoinHandle<()>>,
    workers: usize,
    /// Cap on how many of `workers` a block may actually enrol.
    ///
    /// The pool is sized for an instrument that has the machine to itself;
    /// when several instances play in one host, each one spinning a full
    /// complement of barrier workers oversubscribes the cores and they all
    /// starve — six pianos in a session meant up to fifty real-time threads
    /// on twelve cores. The engine lowers this per block from the count of
    /// ACTIVE instances (see `engine::worker_cap_for`); the threads above the
    /// cap simply stay asleep on the condvar, so an idle cap costs nothing.
    share_cap: usize,
    /// Scratch the audio thread fills before each block.
    pub forces: Vec<f64>,
    pub ears: Vec<(f64, f64)>,
    ears_high: Vec<(f64, f64)>,
    heard: Vec<(f64, f64)>,
    weights: Vec<f64>,
    spent: Vec<f64>,
    /// Block-mode buffers: per-participant ear rows and tile scratch. Sized
    /// on first use; capacity is retained so the audio thread stops paying.
    block_ears: Vec<(f64, f64)>,
    block_scratch: Vec<f64>,
    /// The single-precision twins the block kernel's fast gather needs: the
    /// force matrix (filled by each participant for its own voices, so no
    /// extra pass) and one tile buffer per participant.
    block_forces32: Vec<f32>,
    block_scratch32: Vec<f32>,
}

impl VoicePool {
    /// Spawn `workers` threads. Zero means everything stays on the audio
    /// thread, which is also the fallback wherever threads cannot be had.
    pub fn new(workers: usize, max_voices: usize) -> Self {
        Self::with_scheduling(workers, max_voices, Scheduling::of_current_thread())
    }

    /// As `new`, with the scheduling the workers should adopt — normally the
    /// audio thread's, read at the call site.
    pub fn with_scheduling(workers: usize, max_voices: usize, sched: Scheduling) -> Self {
        let control = Arc::new(Control {
            epoch: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            job: Mutex::new(Job::EMPTY),
            lock: Mutex::new(0),
            cv: Condvar::new(),
        });
        let barrier = Arc::new(SpinBarrier::new(workers + 1));
        let mut handles = Vec::with_capacity(workers);
        for k in 0..workers {
            let control = Arc::clone(&control);
            let barrier = Arc::clone(&barrier);
            handles.push(
                std::thread::Builder::new()
                    .name(format!("piano-voice-{k}"))
                    .spawn(move || {
                        sched.apply_to_current_thread();
                        worker_loop(k + 1, control, barrier)
                    })
                    .expect("a worker thread"),
            );
        }
        Self {
            control,
            barrier,
            handles,
            workers,
            share_cap: usize::MAX,
            forces: vec![0.0; max_voices],
            ears: vec![(0.0, 0.0); workers + 1],
            ears_high: vec![(0.0, 0.0); workers + 1],
            heard: vec![(0.0, 0.0); workers + 1],
            weights: vec![1.0; workers + 1],
            spent: vec![0.0; workers + 1],
            block_ears: Vec::new(),
            block_scratch: Vec::new(),
            block_forces32: Vec::new(),
            block_scratch32: Vec::new(),
        }
    }

    pub fn workers(&self) -> usize {
        self.workers
    }

    /// How many workers a block may actually use, after the multi-instance cap.
    pub fn effective_workers(&self) -> usize {
        self.workers.min(self.share_cap)
    }

    /// See `share_cap`. Cheap; call it every block.
    pub fn set_share_cap(&mut self, cap: usize) {
        self.share_cap = cap;
    }

    /// Run a whole block's string and plate work across the pool.
    ///
    /// `out_ears` comes back with the two ear VELOCITIES for each sample, in
    /// order. What the audio thread does with them afterwards — the radiation
    /// filter, the mechanism, the sampled voices, the output — feeds nothing
    /// back into the strings or the plate, so it does not belong inside the
    /// parallel region at all. Keeping it out saves a barrier per sample and
    /// leaves the whole tail exactly as serial, and as deterministic, as it was.
    pub fn run_block(
        &mut self,
        voices: &mut [crate::voice::Voice],
        board: &mut crate::soundboard::Soundboard,
        live_idx: &[usize],
        shapes: &[std::sync::Arc<Vec<f64>>],
        out_ears: &mut [[f64; 6]],
    ) {
        let parts = participants_for(live_idx.len(), self.effective_workers());
        let frames = out_ears.len();
        let job = Job {
            voices: voices.as_mut_ptr(),
            board: board as *const _,
            forces: self.forces.as_mut_ptr(),
            idx: live_idx.as_ptr(),
            shapes: shapes.as_ptr(),
            n_live: live_idx.len(),
            ears: self.ears.as_mut_ptr(),
            ears_high: self.ears_high.as_mut_ptr(),
            heard: self.heard.as_mut_ptr(),
            weights: self.weights.as_ptr(),
            spent: self.spent.as_mut_ptr(),
            frames,
            parts,
            kind: 0,
            bears: std::ptr::null_mut(),
            scratch: std::ptr::null_mut(),
            forces32: std::ptr::null_mut(),
            scratch32: std::ptr::null_mut(),
            shapes_f32: std::ptr::null(),
        };
        {
            let mut slot = self.control.job.lock().unwrap();
            *slot = job;
        }
        self.barrier.set_total(parts);
        {
            let _g = self.control.lock.lock().unwrap();
            self.control.epoch.fetch_add(1, Ordering::Release);
        }
        self.control.cv.notify_all();

        // The audio thread is participant 0 and takes a share like the rest.
        let mut mine = 0.0f64;
        for out in out_ears.iter_mut() {
            let t = std::time::Instant::now();
            unsafe { voice_phase(0, &job) };
            mine += t.elapsed().as_secs_f64();
            self.barrier.wait();
            // What the voices' microphones heard is summed here, between the
            // barriers: every part's voice phase is complete, and none writes
            // its slot again before the second barrier lets it into the next
            // frame's voice phase.
            let mut acc = [0.0f64; 6];
            for k in 0..parts {
                acc[4] += self.heard[k].0;
                acc[5] += self.heard[k].1;
            }
            unsafe { plate_phase(0, &job) };
            self.barrier.wait();
            for k in 0..parts {
                acc[0] += self.ears[k].0;
                acc[1] += self.ears[k].1;
                acc[2] += self.ears_high[k].0;
                acc[3] += self.ears_high[k].1;
            }
            *out = acc;
        }
        self.spent[0] = mine;
        self.rebalance(live_idx.len(), parts);
    }

    /// Run a whole DECOUPLED block across the pool: each participant rolls
    /// its voice share's forces to the block's end (no board read exists to
    /// order them), one barrier, then advances its slice of the plate's modes
    /// for the whole block into its own ear row. Two barriers per BLOCK — the
    /// per-sample pool pays two per SAMPLE, which is why it lost to one
    /// serial thread once the board went block-major.
    ///
    /// `forces` is the voice-major force matrix (`n_live * frames`), filled
    /// here. `out_ears` comes back scaled, exactly as `run_block`'s does.
    #[allow(clippy::too_many_arguments)]
    pub fn run_block_decoupled(
        &mut self,
        voices: &mut [crate::voice::Voice],
        board: &mut crate::soundboard::Soundboard,
        live_idx: &[usize],
        shapes: &[std::sync::Arc<Vec<f32>>],
        forces: &mut Vec<f64>,
        out_ears: &mut [(f64, f64)],
        sr: f64,
    ) {
        let parts = participants_for(live_idx.len(), self.effective_workers());
        let frames = out_ears.len();
        forces.clear();
        forces.resize(live_idx.len() * frames, 0.0);
        self.block_ears.clear();
        self.block_ears.resize(parts * frames, (0.0, 0.0));
        let tile = crate::modal_bank::BLOCK_TILE * frames;
        self.block_scratch.clear();
        self.block_scratch.resize(parts * tile, 0.0);
        self.block_scratch32.clear();
        self.block_scratch32.resize(parts * tile, 0.0);
        self.block_forces32.clear();
        self.block_forces32.resize(live_idx.len() * frames, 0.0);
        let job = Job {
            voices: voices.as_mut_ptr(),
            board: board as *const _,
            forces: forces.as_mut_ptr(),
            idx: live_idx.as_ptr(),
            // The per-sample path's dense shapes are unused in block mode.
            shapes: std::ptr::null(),
            n_live: live_idx.len(),
            ears: self.ears.as_mut_ptr(),
            ears_high: std::ptr::null_mut(),
            heard: std::ptr::null_mut(),
            weights: self.weights.as_ptr(),
            spent: self.spent.as_mut_ptr(),
            frames,
            parts,
            kind: 1,
            bears: self.block_ears.as_mut_ptr(),
            scratch: self.block_scratch.as_mut_ptr(),
            forces32: self.block_forces32.as_mut_ptr(),
            scratch32: self.block_scratch32.as_mut_ptr(),
            shapes_f32: shapes.as_ptr(),
        };
        {
            let mut slot = self.control.job.lock().unwrap();
            *slot = job;
        }
        self.barrier.set_total(parts);
        {
            let _g = self.control.lock.lock().unwrap();
            self.control.epoch.fetch_add(1, Ordering::Release);
        }
        self.control.cv.notify_all();

        // The audio thread is participant 0 and takes a share like the rest.
        unsafe { voice_block_phase(0, &job) };
        self.barrier.wait();
        unsafe { plate_block_phase(0, &job) };
        self.barrier.wait();
        for o in out_ears.iter_mut() {
            *o = (0.0, 0.0);
        }
        for k in 0..parts {
            let row = &self.block_ears[k * frames..(k + 1) * frames];
            for (o, e) in out_ears.iter_mut().zip(row.iter()) {
                o.0 += e.0;
                o.1 += e.1;
            }
        }
        for o in out_ears.iter_mut() {
            o.0 *= sr;
            o.1 *= sr;
        }
    }

    /// Re-weight the shares from what each participant actually got through.
    ///
    /// Throughput, not time: a participant that was given more voices and took
    /// longer is not slow. Smoothed, because a single block can be disturbed by
    /// anything else the machine is doing, and clamped so one bad block cannot
    /// starve a core outright.
    fn rebalance(&mut self, n_live: usize, parts: usize) {
        if n_live < parts * 2 {
            return;
        }
        let mut fresh = vec![0.0f64; parts];
        let mut any = false;
        for k in 0..parts {
            let (lo, hi) = share_weighted(n_live, k, &self.weights[..parts]);
            let got = (hi - lo) as f64;
            let spent = self.spent[k];
            if got > 0.0 && spent > 1e-9 {
                fresh[k] = got / spent;
                any = true;
            }
        }
        if !any {
            return;
        }
        let mean: f64 = fresh.iter().sum::<f64>() / parts as f64;
        if mean <= 0.0 {
            return;
        }
        for k in 0..parts {
            let target = if fresh[k] > 0.0 { fresh[k] / mean } else { 1.0 };
            // A slow block moves the weight a third of the way, so the split
            // settles over a handful of blocks instead of chasing noise.
            self.weights[k] = (self.weights[k] * 0.67 + target * 0.33).clamp(0.15, 4.0);
        }
    }
}

/// One participant's share of the strings, for one sample.
///
/// # Safety
/// `job` must be live and the participant's range disjoint from every other's.
unsafe fn voice_phase(k: usize, job: &Job) {
    let weights = unsafe { std::slice::from_raw_parts(job.weights, job.parts) };
    let (lo, hi) = share_weighted(job.n_live, k, weights);
    let mut heard = (0.0f64, 0.0f64);
    for s in lo..hi {
        let vi = unsafe { *job.idx.add(s) };
        let v = unsafe { &mut *job.voices.add(vi) };
        // A voice can retire part way through a block; the serial loop skips
        // it from that sample on, and so does this.
        if !v.active {
            unsafe { *job.forces.add(s) = 0.0 };
            continue;
        }
        let board = unsafe { &*job.board };
        let (y, c) = board.read_and_compliance_at(&v.attach);
        let f = v.tick(y, c);
        unsafe { *job.forces.add(s) = f };
        let (l, r) = v.heard();
        heard.0 += l;
        heard.1 += r;
    }
    if !job.heard.is_null() {
        unsafe { *job.heard.add(k) = heard };
    }
}

/// One participant's share of the plate, for one sample.
///
/// # Safety
/// As above; the mode ranges tile the live modes and never overlap.
unsafe fn plate_phase(k: usize, job: &Job) {
    let board = unsafe { &*job.board };
    let (lo, hi) = share(board.live(), k, job.parts);
    let shapes = unsafe { std::slice::from_raw_parts(job.shapes, job.n_live) };
    let forces = unsafe { std::slice::from_raw_parts(job.forces, job.n_live) };
    let e = unsafe { board.range_advance_split(lo, hi, shapes, forces) };
    unsafe { *job.ears.add(k) = (e[0], e[1]) };
    unsafe { *job.ears_high.add(k) = (e[2], e[3]) };
}

/// One participant's share of the strings, for the WHOLE block: every voice
/// in its share is rolled to the block's end, its forces into its row of the
/// voice-major matrix. Decoupled only — the voices read nothing back, which
/// is exactly what makes the roll-out legal.
///
/// # Safety
/// As `voice_phase`: the participant's voice share is disjoint from every
/// other's, and `job` must be live.
unsafe fn voice_block_phase(k: usize, job: &Job) {
    let (lo, hi) = share(job.n_live, k, job.parts);
    let frames = job.frames;
    for s in lo..hi {
        let vi = unsafe { *job.idx.add(s) };
        let v = unsafe { &mut *job.voices.add(vi) };
        let row = unsafe { std::slice::from_raw_parts_mut(job.forces.add(s * frames), frames) };
        // The single-precision twin is written here, by the participant that
        // owns the voice: the gather wants f32 and this costs no extra pass.
        let row32 =
            unsafe { std::slice::from_raw_parts_mut(job.forces32.add(s * frames), frames) };
        for t in 0..frames {
            if !v.active {
                break;
            }
            let f = v.tick(0.0, 0.0);
            row[t] = f;
            row32[t] = f as f32;
        }
    }
}

/// One participant's slice of the plate, for the WHOLE block, into its own
/// ear row (summed and scaled by the audio thread afterwards).
///
/// # Safety
/// As `plate_phase`: the mode ranges tile the live modes and never overlap,
/// and every pointer in `job` must be live.
unsafe fn plate_block_phase(k: usize, job: &Job) {
    let board = unsafe { &*job.board };
    let (lo, hi) = share(board.live(), k, job.parts);
    let frames = job.frames;
    let shapes = unsafe { std::slice::from_raw_parts(job.shapes_f32, job.n_live) };
    let forces = unsafe { std::slice::from_raw_parts(job.forces, job.n_live * frames) };
    let out = unsafe { std::slice::from_raw_parts_mut(job.bears.add(k * frames), frames) };
    let tile = crate::modal_bank::BLOCK_TILE * frames;
    let scratch = unsafe { std::slice::from_raw_parts_mut(job.scratch.add(k * tile), tile) };
    let scratch32 =
        unsafe { std::slice::from_raw_parts_mut(job.scratch32.add(k * tile), tile) };
    let forces32 =
        unsafe { std::slice::from_raw_parts(job.forces32 as *const f32, job.n_live * frames) };
    unsafe {
        board.range_advance_block(
            lo, hi, shapes, forces, forces32, frames, out, scratch, scratch32,
        )
    };
}

fn worker_loop(k: usize, control: Arc<Control>, barrier: Arc<SpinBarrier>) {
    let mut seen = 0usize;
    loop {
        // Park between blocks: an idle plugin must not burn a core.
        {
            let mut g = control.lock.lock().unwrap();
            while control.epoch.load(Ordering::Acquire) == seen
                && !control.stop.load(Ordering::Acquire)
            {
                g = control.cv.wait(g).unwrap();
            }
            seen = control.epoch.load(Ordering::Acquire);
        }
        if control.stop.load(Ordering::Acquire) {
            return;
        }
        let job = *control.job.lock().unwrap();
        if k >= job.parts {
            // Not needed this block: sit the whole thing out rather than
            // arriving at barriers nobody is waiting for.
            continue;
        }
        if job.kind == 1 {
            unsafe { voice_block_phase(k, &job) };
            barrier.wait();
            unsafe { plate_block_phase(k, &job) };
            barrier.wait();
            continue;
        }
        let mut mine = 0.0f64;
        for _ in 0..job.frames {
            let t = std::time::Instant::now();
            unsafe { voice_phase(k, &job) };
            mine += t.elapsed().as_secs_f64();
            barrier.wait();
            unsafe { plate_phase(k, &job) };
            barrier.wait();
        }
        unsafe { *job.spent.add(k) = mine };
    }
}

impl Drop for VoicePool {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::Release);
        {
            let _g = self.control.lock.lock().unwrap();
            self.control.epoch.fetch_add(1, Ordering::Release);
        }
        self.control.cv.notify_all();
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

/// Which slice of `n` items belongs to participant `k`, given what each
/// participant has been getting through.
///
/// Equal shares are wrong on this class of machine. A recent laptop part has
/// cores of two or three different speeds — two at 4.9 GHz, eight at 3.8, two
/// at 2.1 on the machine this was written on — and a barrier makes every
/// participant wait for the slowest. Handing the fast cores more voices is the
/// difference between scaling and not: with equal shares this pool stopped
/// improving past four workers.
///
/// `weights` are relative throughputs, and the split is by cumulative weight,
/// which keeps it contiguous and deterministic for a given weight vector.
pub fn share_weighted(n: usize, k: usize, weights: &[f64]) -> (usize, usize) {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 || weights.is_empty() {
        return share(n, k, weights.len().max(1));
    }
    let before: f64 = weights[..k].iter().sum();
    let upto: f64 = before + weights[k];
    let lo = ((before / total) * n as f64).round() as usize;
    let hi = ((upto / total) * n as f64).round() as usize;
    let lo = lo.min(n);
    let hi = hi.max(lo).min(n);
    // The last participant always closes the range, whatever rounding did.
    if k + 1 == weights.len() {
        (lo, n)
    } else {
        (lo, hi)
    }
}

/// Which slice of `n` items belongs to participant `k` of `w`.
///
/// Contiguous and deterministic: the same split every time, so a voice always
/// lands on the same participant and the partial sums are always combined in
/// the same order. Balanced to within one item, because everyone waits for the
/// slowest.
pub fn share(n: usize, k: usize, w: usize) -> (usize, usize) {
    let w = w.max(1);
    let base = n / w;
    let extra = n % w;
    let start = k * base + k.min(extra);
    let len = base + if k < extra { 1 } else { 0 };
    (start, start + len)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The split must cover everything exactly once, whatever the shapes.
    #[test]
    fn the_shares_tile_the_work() {
        for n in [0usize, 1, 5, 88, 192] {
            for w in 1usize..=13 {
                let mut covered = vec![0u8; n];
                let mut last_end = 0;
                for k in 0..w {
                    let (a, b) = share(n, k, w);
                    assert!(a <= b, "backwards share at n={n} w={w} k={k}");
                    assert_eq!(a, last_end, "gap or overlap at n={n} w={w} k={k}");
                    for c in covered.iter_mut().take(b).skip(a) {
                        *c += 1;
                    }
                    last_end = b;
                }
                assert_eq!(last_end, n, "the last share does not reach the end");
                assert!(covered.iter().all(|&c| c == 1), "not covered exactly once");
            }
        }
    }

    /// Shares must be within one of each other, or one core finishes late and
    /// everyone waits for it.
    #[test]
    fn the_shares_are_balanced() {
        for n in [7usize, 31, 88] {
            for w in 2usize..=12 {
                let lens: Vec<usize> = (0..w).map(|k| { let (a, b) = share(n, k, w); b - a }).collect();
                let (lo, hi) = (lens.iter().min().unwrap(), lens.iter().max().unwrap());
                assert!(hi - lo <= 1, "unbalanced at n={n} w={w}: {lens:?}");
            }
        }
    }

    /// The barrier must let everyone through, every pass, with no reset.
    #[test]
    fn the_barrier_releases_every_pass() {
        const W: usize = 4;
        const PASSES: usize = 2000;
        let b = Arc::new(SpinBarrier::new(W));
        let seen = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|s| {
            for _ in 0..W {
                let b = Arc::clone(&b);
                let seen = Arc::clone(&seen);
                s.spawn(move || {
                    for _ in 0..PASSES {
                        seen.fetch_add(1, Ordering::Relaxed);
                        b.wait();
                    }
                });
            }
        });
        assert_eq!(seen.load(Ordering::Relaxed), W * PASSES);
    }
}
