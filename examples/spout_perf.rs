//! Spout sender performance probe for the CPU DX11 and experimental GPU DX12 paths.

#[cfg(all(windows, feature = "cpu-dx11", feature = "gpu-dx12-experimental"))]
fn main() {
    if let Err(err) = perf::run() {
        eprintln!("spout_perf: {err}");
        std::process::exit(2);
    }
}

#[cfg(not(all(windows, feature = "cpu-dx11", feature = "gpu-dx12-experimental")))]
fn main() {
    eprintln!(
        "This example requires Windows with the `cpu-dx11` and \
         `gpu-dx12-experimental` features."
    );
}

#[cfg(all(windows, feature = "cpu-dx11", feature = "gpu-dx12-experimental"))]
mod perf {
    use nanalive_spout::{
        CpuDx11Sender, GpuDx12ExperimentalSender, GpuDx12PublishOptions, SpoutFormat,
        SpoutFrameRef, SpoutPublishStatus, SpoutSenderBackend,
    };
    use std::error::Error;
    use std::ffi::c_void;
    use std::fmt;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::Common::*;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::Win32::System::Performance::*;
    use windows::Win32::System::Threading::{
        CreateEventW, GetCurrentProcess, GetProcessTimes, INFINITE, WaitForSingleObject,
    };
    use windows::core::{Interface, PCWSTR};

    type AppResult<T> = Result<T, Box<dyn Error>>;

    pub fn run() -> AppResult<()> {
        let config = Config::parse()?;
        let mut results = Vec::new();

        if config.mode.includes_cpu() {
            match run_cpu(&config) {
                Ok(summary) => results.push(summary),
                Err(err) => eprintln!("cpu path failed: {err}"),
            }
        }
        if config.mode.includes_gpu() {
            match unsafe { run_gpu_dx12(&config) } {
                Ok(summary) => results.push(summary),
                Err(err) => eprintln!("gpu-dx12 path failed: {err}"),
            }
        }

        if results.is_empty() {
            return Err("no path completed successfully".into());
        }

        if config.csv {
            print_csv(&results);
        } else {
            print_table(&config, &results);
        }
        Ok(())
    }

    #[derive(Debug, Clone)]
    struct Config {
        mode: Mode,
        width: u32,
        height: u32,
        frames: u32,
        warmup: u32,
        name: String,
        csv: bool,
    }

    impl Config {
        fn parse() -> AppResult<Self> {
            let mut config = Self {
                mode: Mode::Both,
                width: 1280,
                height: 720,
                frames: 600,
                warmup: 60,
                name: "nanalive-spout-perf".to_string(),
                csv: false,
            };

            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--mode" => config.mode = parse_value(&arg, args.next())?,
                    "--width" => config.width = parse_value(&arg, args.next())?,
                    "--height" => config.height = parse_value(&arg, args.next())?,
                    "--frames" => config.frames = parse_value(&arg, args.next())?,
                    "--warmup" => config.warmup = parse_value(&arg, args.next())?,
                    "--name" => config.name = parse_value(&arg, args.next())?,
                    "--csv" => config.csv = true,
                    "--help" | "-h" => {
                        print_usage();
                        std::process::exit(0);
                    }
                    _ => return Err(format!("unknown argument `{arg}`").into()),
                }
            }

            if config.width == 0 || config.height == 0 {
                return Err("width and height must be greater than zero".into());
            }
            if config.frames == 0 {
                return Err("frames must be greater than zero".into());
            }
            Ok(config)
        }
    }

    fn parse_value<T>(flag: &str, value: Option<String>) -> AppResult<T>
    where
        T: std::str::FromStr,
        T::Err: Error + 'static,
    {
        value
            .ok_or_else(|| format!("missing value for `{flag}`"))?
            .parse::<T>()
            .map_err(|err| Box::new(err) as Box<dyn Error>)
    }

    fn print_usage() {
        println!(
            "Usage: cargo run --example spout_perf --features gpu-dx12-experimental -- \
             [--mode both|cpu|gpu-dx12] [--width 1280] [--height 720] \
             [--frames 600] [--warmup 60] [--name sender-name] [--csv]"
        );
    }

    #[derive(Debug, Clone, Copy)]
    enum Mode {
        Both,
        Cpu,
        GpuDx12,
    }

    impl Mode {
        fn includes_cpu(self) -> bool {
            matches!(self, Self::Both | Self::Cpu)
        }

        fn includes_gpu(self) -> bool {
            matches!(self, Self::Both | Self::GpuDx12)
        }
    }

    impl std::str::FromStr for Mode {
        type Err = ParseModeError;

        fn from_str(value: &str) -> Result<Self, Self::Err> {
            match value {
                "both" => Ok(Self::Both),
                "cpu" => Ok(Self::Cpu),
                "gpu-dx12" => Ok(Self::GpuDx12),
                _ => Err(ParseModeError(value.to_string())),
            }
        }
    }

    #[derive(Debug)]
    struct ParseModeError(String);

    impl fmt::Display for ParseModeError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "invalid mode `{}`; use both, cpu, or gpu-dx12", self.0)
        }
    }

    impl Error for ParseModeError {}

    fn run_cpu(config: &Config) -> AppResult<PathSummary> {
        let mut sender = CpuDx11Sender::new(&format!("{}-cpu", config.name))?;
        sender.resize_or_recreate(config.width, config.height, SpoutFormat::default())?;

        let len = (config.width as usize)
            .checked_mul(config.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("frame size is too large")?;
        let mut pixels = vec![0u8; len];
        for px in pixels.chunks_exact_mut(4) {
            px[3] = 255;
        }

        let mut recorder = Recorder::new("cpu", config);
        for frame in 0..config.warmup {
            touch_cpu_pixels(&mut pixels, config.width, config.height, frame);
            let _ = sender.publish(SpoutFrameRef::CpuPixels {
                pixels: &pixels,
                width: config.width,
                height: config.height,
                pitch_bytes: None,
            });
        }
        recorder.reset_window();

        for frame in 0..config.frames {
            touch_cpu_pixels(
                &mut pixels,
                config.width,
                config.height,
                frame + config.warmup,
            );
            let start = Instant::now();
            let result = sender.publish(SpoutFrameRef::CpuPixels {
                pixels: &pixels,
                width: config.width,
                height: config.height,
                pitch_bytes: None,
            });
            let elapsed = start.elapsed();
            recorder.record(elapsed, result.is_ok());
        }

        Ok(recorder.finish(sender.status()))
    }

    fn touch_cpu_pixels(pixels: &mut [u8], width: u32, height: u32, frame: u32) {
        let x = frame % width;
        let y = (frame / width) % height;
        let i = ((y * width + x) * 4) as usize;
        pixels[i] = frame as u8;
        pixels[i + 1] = frame.wrapping_mul(3) as u8;
        pixels[i + 2] = frame.wrapping_mul(7) as u8;
    }

    unsafe fn run_gpu_dx12(config: &Config) -> AppResult<PathSummary> {
        let mut dx = unsafe { Dx12Bench::new(config.width, config.height)? };
        let mut sender = unsafe {
            GpuDx12ExperimentalSender::with_d3d12_device_and_queue(
                &format!("{}-gpu-dx12", config.name),
                dx.device.as_raw() as *mut c_void,
                dx.queue.as_raw() as *mut c_void,
            )?
        };
        sender.resize_or_recreate(config.width, config.height, SpoutFormat::R8G8B8A8_UNORM)?;
        sender.set_publish_options(GpuDx12PublishOptions::default());

        let mut recorder = Recorder::new("gpu-dx12", config);
        for frame in 0..config.warmup {
            unsafe { dx.clear(frame)? };
            let _ = sender.publish_report(SpoutFrameRef::Dx12Resource {
                resource: dx.texture.as_raw() as *mut c_void,
                initial_state: D3D12_RESOURCE_STATE_RENDER_TARGET.0 as u32,
                final_state: D3D12_RESOURCE_STATE_RENDER_TARGET.0 as u32,
            });
        }
        recorder.reset_window();

        for frame in 0..config.frames {
            unsafe { dx.clear(frame + config.warmup)? };
            let start = Instant::now();
            let result = sender.publish_report(SpoutFrameRef::Dx12Resource {
                resource: dx.texture.as_raw() as *mut c_void,
                initial_state: D3D12_RESOURCE_STATE_RENDER_TARGET.0 as u32,
                final_state: D3D12_RESOURCE_STATE_RENDER_TARGET.0 as u32,
            });
            let elapsed = start.elapsed();
            recorder.record(
                elapsed,
                matches!(result, Ok(report) if report.status == SpoutPublishStatus::Sent),
            );
        }

        Ok(recorder.finish(sender.status()))
    }

    struct Dx12Bench {
        device: ID3D12Device,
        queue: ID3D12CommandQueue,
        allocator: ID3D12CommandAllocator,
        list: ID3D12GraphicsCommandList,
        texture: ID3D12Resource,
        _rtv_heap: ID3D12DescriptorHeap,
        rtv_handle: D3D12_CPU_DESCRIPTOR_HANDLE,
        fence: ID3D12Fence,
        fence_event: HANDLE,
        fence_value: u64,
    }

    impl Dx12Bench {
        unsafe fn new(width: u32, height: u32) -> windows::core::Result<Self> {
            let factory: IDXGIFactory4 =
                unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
            let adapter = unsafe { first_hardware_adapter(&factory)? };
            let mut device = None;
            unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device)? };
            let device: ID3D12Device = device.unwrap();

            let queue_desc = D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            };
            let queue = unsafe { device.CreateCommandQueue(&queue_desc)? };
            let allocator =
                unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)? };
            let list: ID3D12GraphicsCommandList = unsafe {
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?
            };
            unsafe { list.Close()? };

            let rtv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 0,
            };
            let rtv_heap: ID3D12DescriptorHeap =
                unsafe { device.CreateDescriptorHeap(&rtv_heap_desc)? };
            let rtv_handle = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };

            let clear_value = D3D12_CLEAR_VALUE {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                Anonymous: D3D12_CLEAR_VALUE_0 {
                    Color: [0.0, 0.0, 0.0, 1.0],
                },
            };
            let tex_desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                Alignment: 0,
                Width: width as u64,
                Height: height,
                DepthOrArraySize: 1,
                MipLevels: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
                Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            };
            let heap_props = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_DEFAULT,
                ..Default::default()
            };
            let mut texture = None;
            unsafe {
                device.CreateCommittedResource(
                    &heap_props,
                    D3D12_HEAP_FLAG_NONE,
                    &tex_desc,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    Some(&clear_value),
                    &mut texture,
                )?;
            }
            let texture = texture.unwrap();
            unsafe { device.CreateRenderTargetView(&texture, None, rtv_handle) };

            let fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
            let fence_event = unsafe { CreateEventW(None, false, false, None)? };

            Ok(Self {
                device,
                queue,
                allocator,
                list,
                texture,
                _rtv_heap: rtv_heap,
                rtv_handle,
                fence,
                fence_event,
                fence_value: 1,
            })
        }

        unsafe fn clear(&mut self, frame: u32) -> windows::core::Result<()> {
            let color = clear_color(frame);
            unsafe {
                self.allocator.Reset()?;
                self.list.Reset(&self.allocator, None)?;
                self.list
                    .ClearRenderTargetView(self.rtv_handle, &color, None);
                self.list.Close()?;
                let lists = [Some(self.list.clone().into())];
                self.queue.ExecuteCommandLists(&lists);
                self.wait_for_gpu()?;
            }
            Ok(())
        }

        unsafe fn wait_for_gpu(&mut self) -> windows::core::Result<()> {
            let value = self.fence_value;
            unsafe {
                self.queue.Signal(&self.fence, value)?;
                if self.fence.GetCompletedValue() < value {
                    self.fence.SetEventOnCompletion(value, self.fence_event)?;
                    let wait = WaitForSingleObject(self.fence_event, INFINITE);
                    if wait != WAIT_OBJECT_0 {
                        return Err(windows::core::Error::from_win32());
                    }
                }
            }
            self.fence_value += 1;
            Ok(())
        }
    }

    impl Drop for Dx12Bench {
        fn drop(&mut self) {
            if !self.fence_event.is_invalid() {
                unsafe {
                    let _ = CloseHandle(self.fence_event);
                }
            }
        }
    }

    unsafe fn first_hardware_adapter(
        factory: &IDXGIFactory4,
    ) -> windows::core::Result<IDXGIAdapter1> {
        let mut index = 0;
        loop {
            let adapter = unsafe { factory.EnumAdapters1(index)? };
            let desc = unsafe { adapter.GetDesc1()? };
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) == 0 {
                return Ok(adapter);
            }
            index += 1;
        }
    }

    fn clear_color(frame: u32) -> [f32; 4] {
        let phase = (frame % 255) as f32 / 255.0;
        [phase, 1.0 - phase, (phase * 0.5) + 0.25, 1.0]
    }

    struct Recorder {
        path: &'static str,
        width: u32,
        height: u32,
        frames: u32,
        warmup: u32,
        latencies: Vec<Duration>,
        failures: u32,
        cpu_start: ProcessCpuSnapshot,
        cpu_end: ProcessCpuSnapshot,
        gpu: GpuUsageSampler,
        start: Instant,
        end: Instant,
    }

    impl Recorder {
        fn new(path: &'static str, config: &Config) -> Self {
            let now = Instant::now();
            Self {
                path,
                width: config.width,
                height: config.height,
                frames: config.frames,
                warmup: config.warmup,
                latencies: Vec::with_capacity(config.frames as usize),
                failures: 0,
                cpu_start: ProcessCpuSnapshot::now(),
                cpu_end: ProcessCpuSnapshot::zero(),
                gpu: GpuUsageSampler::new(std::process::id()),
                start: now,
                end: now,
            }
        }

        fn reset_window(&mut self) {
            self.start = Instant::now();
            self.cpu_start = ProcessCpuSnapshot::now();
            self.gpu = GpuUsageSampler::new(std::process::id());
        }

        fn record(&mut self, elapsed: Duration, ok: bool) {
            if ok {
                self.latencies.push(elapsed);
            } else {
                self.failures += 1;
            }
            self.gpu.sample();
        }

        fn finish(mut self, status: nanalive_spout::SpoutStatus) -> PathSummary {
            self.cpu_end = ProcessCpuSnapshot::now();
            self.end = Instant::now();
            let latency = LatencyStats::from_durations(&self.latencies);
            let wall = self.end.saturating_duration_since(self.start);
            let cpu_percent = self.cpu_start.percent_until(&self.cpu_end, wall);
            let gpu = self.gpu.finish();
            PathSummary {
                path: self.path,
                width: self.width,
                height: self.height,
                frames: self.frames,
                warmup: self.warmup,
                success: self.latencies.len() as u32,
                failures: self.failures,
                latency,
                spout_fps: status.fps,
                spout_frame: status.frame,
                cpu_percent,
                gpu,
            }
        }
    }

    #[derive(Clone)]
    struct PathSummary {
        path: &'static str,
        width: u32,
        height: u32,
        frames: u32,
        warmup: u32,
        success: u32,
        failures: u32,
        latency: LatencyStats,
        spout_fps: Option<f64>,
        spout_frame: Option<i64>,
        cpu_percent: Option<f64>,
        gpu: GpuUsageResult,
    }

    #[derive(Clone)]
    struct LatencyStats {
        mean_ms: Option<f64>,
        p50_ms: Option<f64>,
        p95_ms: Option<f64>,
        p99_ms: Option<f64>,
        min_ms: Option<f64>,
        max_ms: Option<f64>,
    }

    impl LatencyStats {
        fn from_durations(durations: &[Duration]) -> Self {
            if durations.is_empty() {
                return Self {
                    mean_ms: None,
                    p50_ms: None,
                    p95_ms: None,
                    p99_ms: None,
                    min_ms: None,
                    max_ms: None,
                };
            }
            let mut values = durations.iter().map(duration_ms).collect::<Vec<_>>();
            values.sort_by(|a, b| a.total_cmp(b));
            let sum = values.iter().sum::<f64>();
            let mean = sum / values.len() as f64;
            Self {
                mean_ms: Some(mean),
                p50_ms: Some(percentile(&values, 0.50)),
                p95_ms: Some(percentile(&values, 0.95)),
                p99_ms: Some(percentile(&values, 0.99)),
                min_ms: values.first().copied(),
                max_ms: values.last().copied(),
            }
        }
    }

    fn duration_ms(duration: &Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    fn percentile(sorted: &[f64], q: f64) -> f64 {
        let index = ((sorted.len() - 1) as f64 * q).ceil() as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    #[derive(Clone, Copy)]
    struct ProcessCpuSnapshot {
        user_100ns: u64,
        kernel_100ns: u64,
    }

    impl ProcessCpuSnapshot {
        fn zero() -> Self {
            Self {
                user_100ns: 0,
                kernel_100ns: 0,
            }
        }

        fn now() -> Self {
            unsafe {
                let mut creation = FILETIME::default();
                let mut exit = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                if GetProcessTimes(
                    GetCurrentProcess(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
                .is_ok()
                {
                    Self {
                        user_100ns: filetime_to_u64(user),
                        kernel_100ns: filetime_to_u64(kernel),
                    }
                } else {
                    Self::zero()
                }
            }
        }

        fn percent_until(self, end: &Self, wall: Duration) -> Option<f64> {
            let cpu_delta = end
                .user_100ns
                .saturating_add(end.kernel_100ns)
                .saturating_sub(self.user_100ns.saturating_add(self.kernel_100ns));
            if cpu_delta == 0 || wall.is_zero() {
                return Some(0.0);
            }
            let cores = std::thread::available_parallelism()
                .map(|n| n.get() as f64)
                .unwrap_or(1.0);
            let cpu_secs = cpu_delta as f64 / 10_000_000.0;
            Some((cpu_secs / wall.as_secs_f64()) * 100.0 / cores)
        }
    }

    fn filetime_to_u64(filetime: FILETIME) -> u64 {
        ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64
    }

    struct GpuUsageSampler {
        inner: Result<PdhGpuQuery, String>,
        samples: Vec<f64>,
    }

    impl GpuUsageSampler {
        fn new(pid: u32) -> Self {
            Self {
                inner: PdhGpuQuery::new(pid),
                samples: Vec::new(),
            }
        }

        fn sample(&mut self) {
            let Ok(query) = self.inner.as_mut() else {
                return;
            };
            if let Ok(sample) = query.sample() {
                self.samples.push(sample);
            }
        }

        fn finish(self) -> GpuUsageResult {
            match self.inner {
                Ok(_) if self.samples.is_empty() => {
                    GpuUsageResult::Unavailable("no GPU Engine samples matched this process".into())
                }
                Ok(_) => {
                    let avg = self.samples.iter().sum::<f64>() / self.samples.len() as f64;
                    let peak = self.samples.iter().copied().fold(0.0, f64::max);
                    GpuUsageResult::Available { avg, peak }
                }
                Err(reason) => GpuUsageResult::Unavailable(reason),
            }
        }
    }

    struct PdhGpuQuery {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
        pid_marker: String,
    }

    impl PdhGpuQuery {
        fn new(pid: u32) -> Result<Self, String> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                let status = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
                if status != 0 {
                    return Err(format!("PdhOpenQueryW failed: 0x{status:08x}"));
                }

                let mut counter = PDH_HCOUNTER::default();
                let path = wide_null("\\GPU Engine(*)\\Utilization Percentage");
                let status = PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter);
                if status != 0 {
                    let _ = PdhCloseQuery(query);
                    return Err(format!(
                        "GPU Engine utilization counter is unavailable: 0x{status:08x}"
                    ));
                }

                let status = PdhCollectQueryData(query);
                if status != 0 {
                    let _ = PdhCloseQuery(query);
                    return Err(format!("PdhCollectQueryData failed: 0x{status:08x}"));
                }

                Ok(Self {
                    query,
                    counter,
                    pid_marker: format!("pid_{pid}_"),
                })
            }
        }

        fn sample(&mut self) -> Result<f64, String> {
            unsafe {
                let status = PdhCollectQueryData(self.query);
                if status != 0 {
                    return Err(format!("PdhCollectQueryData failed: 0x{status:08x}"));
                }

                let mut bytes = 0u32;
                let mut count = 0u32;
                let status = PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    None,
                );
                if status != PDH_MORE_DATA {
                    return Err(format!(
                        "PdhGetFormattedCounterArrayW sizing failed: 0x{status:08x}"
                    ));
                }

                let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let mut items =
                    vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); bytes as usize / item_size + 1];
                let status = PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    Some(items.as_mut_ptr()),
                );
                if status != 0 {
                    return Err(format!(
                        "PdhGetFormattedCounterArrayW failed: 0x{status:08x}"
                    ));
                }

                let mut total = 0.0;
                for item in items.iter().take(count as usize) {
                    let name = pwstr_to_string(item.szName);
                    if name.contains(&self.pid_marker) && item.FmtValue.CStatus == 0 {
                        total += item.FmtValue.Anonymous.doubleValue;
                    }
                }
                Ok(total)
            }
        }
    }

    impl Drop for PdhGpuQuery {
        fn drop(&mut self) {
            unsafe {
                let _ = PdhCloseQuery(self.query);
            }
        }
    }

    #[derive(Clone)]
    enum GpuUsageResult {
        Available { avg: f64, peak: f64 },
        Unavailable(String),
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn pwstr_to_string(value: windows::core::PWSTR) -> String {
        if value.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while unsafe { *value.0.add(len) } != 0 {
            len += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value.0, len) })
    }

    fn print_table(config: &Config, results: &[PathSummary]) {
        println!(
            "Spout performance: {}x{}, frames {}, warmup {}",
            config.width, config.height, config.frames, config.warmup
        );
        println!(
            "{:<10} {:>7} {:>6} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>10} {:>10}",
            "path",
            "success",
            "fail",
            "mean_ms",
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "min_ms",
            "max_ms",
            "cpu%",
            "gpu%",
            "spout_fps",
            "spout_frame"
        );
        for result in results {
            println!(
                "{:<10} {:>7} {:>6} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>10} {:>10}",
                result.path,
                result.success,
                result.failures,
                fmt_opt(result.latency.mean_ms),
                fmt_opt(result.latency.p50_ms),
                fmt_opt(result.latency.p95_ms),
                fmt_opt(result.latency.p99_ms),
                fmt_opt(result.latency.min_ms),
                fmt_opt(result.latency.max_ms),
                fmt_opt(result.cpu_percent),
                fmt_gpu_avg(&result.gpu),
                fmt_opt(result.spout_fps),
                result
                    .spout_frame
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "n/a".to_string())
            );
            if let GpuUsageResult::Unavailable(reason) = &result.gpu {
                println!("  {} gpu%: n/a ({reason})", result.path);
            } else if let GpuUsageResult::Available { peak, .. } = result.gpu {
                println!("  {} peak gpu%: {:.2}", result.path, peak);
            }
        }
    }

    fn print_csv(results: &[PathSummary]) {
        println!(
            "path,width,height,frames,warmup,success,failures,mean_ms,p50_ms,p95_ms,p99_ms,min_ms,max_ms,cpu_percent,gpu_avg_percent,gpu_peak_percent,spout_fps,spout_frame,gpu_note"
        );
        for result in results {
            let (gpu_avg, gpu_peak, note) = match &result.gpu {
                GpuUsageResult::Available { avg, peak } => {
                    (format!("{avg:.6}"), format!("{peak:.6}"), String::new())
                }
                GpuUsageResult::Unavailable(reason) => {
                    (String::new(), String::new(), escape_csv(reason))
                }
            };
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                result.path,
                result.width,
                result.height,
                result.frames,
                result.warmup,
                result.success,
                result.failures,
                fmt_csv(result.latency.mean_ms),
                fmt_csv(result.latency.p50_ms),
                fmt_csv(result.latency.p95_ms),
                fmt_csv(result.latency.p99_ms),
                fmt_csv(result.latency.min_ms),
                fmt_csv(result.latency.max_ms),
                fmt_csv(result.cpu_percent),
                gpu_avg,
                gpu_peak,
                fmt_csv(result.spout_fps),
                result
                    .spout_frame
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                note
            );
        }
    }

    fn fmt_opt(value: Option<f64>) -> String {
        value
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "n/a".to_string())
    }

    fn fmt_csv(value: Option<f64>) -> String {
        value.map(|v| format!("{v:.6}")).unwrap_or_default()
    }

    fn fmt_gpu_avg(value: &GpuUsageResult) -> String {
        match value {
            GpuUsageResult::Available { avg, .. } => format!("{avg:.2}"),
            GpuUsageResult::Unavailable(_) => "n/a".to_string(),
        }
    }

    fn escape_csv(value: &str) -> String {
        if value.contains([',', '"', '\n']) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }
}
