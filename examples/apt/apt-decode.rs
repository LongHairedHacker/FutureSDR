extern crate image;

use async_trait::async_trait;
use std::mem::size_of;

use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::FileSource;
use futuresdr::runtime::AsyncKernel;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::new("apt.mp3");
    //let inner = src.as_async::<FileSource>().unwrap();

    let amdemod = AMDemodulator::new();

    // Lowpass with 4160Hz cutoff
    let lowpass_coeffs = vec![
        -7.383784e-03,
        -3.183046e-03,
        2.255039e-03,
        7.461166e-03,
        1.091908e-02,
        1.149109e-02,
        8.769802e-03,
        3.252932e-03,
        -3.720606e-03,
        -1.027446e-02,
        -1.447403e-02,
        -1.486427e-02,
        -1.092423e-02,
        -3.307958e-03,
        6.212477e-03,
        1.511364e-02,
        2.072873e-02,
        2.096037e-02,
        1.492345e-02,
        3.347624e-03,
        -1.138407e-02,
        -2.560252e-02,
        -3.507114e-02,
        -3.591225e-02,
        -2.553830e-02,
        -3.371569e-03,
        2.882645e-02,
        6.711368e-02,
        1.060042e-01,
        1.394643e-01,
        1.620650e-01,
        1.700462e-01,
        1.620650e-01,
        1.394643e-01,
        1.060042e-01,
        6.711368e-02,
        2.882645e-02,
        -3.371569e-03,
        -2.553830e-02,
        -3.591225e-02,
        -3.507114e-02,
        -2.560252e-02,
        -1.138407e-02,
        3.347624e-03,
        1.492345e-02,
        2.096037e-02,
        2.072873e-02,
        1.511364e-02,
        6.212477e-03,
        -3.307958e-03,
        -1.092423e-02,
        -1.486427e-02,
        -1.447403e-02,
        -1.027446e-02,
        -3.720606e-03,
        3.252932e-03,
        8.769802e-03,
        1.149109e-02,
        1.091908e-02,
        7.461166e-03,
        2.255039e-03,
        -3.183046e-03,
        -7.383784e-03,
    ];

    let lowpass = FIRFilter::new(lowpass_coeffs);

    let upsampler = Upsampler::new(13);
    let downsampler = Downsampler::new(150);

    let snk = APTImageSink::new("apt.png".to_string());

    let src = fg.add_block(src);
    let amdemod = fg.add_block(amdemod);
    let lowpass = fg.add_block(lowpass);
    let upsampler = fg.add_block(upsampler);
    let downsampler = fg.add_block(downsampler);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", amdemod, "in")?;
    fg.connect_stream(amdemod, "out", lowpass, "in")?;
    fg.connect_stream(lowpass, "out", upsampler, "in")?;
    fg.connect_stream(upsampler, "out", downsampler, "in")?;
    fg.connect_stream(downsampler, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}

pub struct AMDemodulator {}

impl AMDemodulator {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("AMDemodulator").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {},
        )
    }
}

#[async_trait]
impl AsyncKernel for AMDemodulator {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len());

        for x in 0..n {
            o[x] = (i[x] * i[x]).sqrt()
        }

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);
        sio.output(0).produce(n);

        Ok(())
    }
}

pub struct FIRFilter {
    // coeffs could probably just be a reference to an array,
    // but I don't want to bother with the resulting lifetime issues (yet)
    coeffs: Vec<f32>,
    state: Vec<f32>,
    pos: usize,
}

impl FIRFilter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(coeffs: Vec<f32>) -> Block {
        let mut state = Vec::new();
        for _ in 0..coeffs.len() {
            state.push(0.0);
        }

        Block::new_async(
            BlockMetaBuilder::new("FIRFilter").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                pos: 0,
                coeffs,
                state,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for FIRFilter {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len());

        for x in 0..n {
            // Keeping the delayed samples in state can probably be avoided
            self.pos = (self.pos + 1) % self.coeffs.len();
            self.state[self.pos] = i[x];
            o[x] = 0.0;
            for i in 0..self.coeffs.len() {
                let pos = (self.pos + self.coeffs.len() - i) % self.coeffs.len();
                o[x] += self.state[pos] * self.coeffs[i];
            }
        }

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);
        sio.output(0).produce(n);

        Ok(())
    }
}

pub struct Upsampler {
    rate: usize,
}

impl Upsampler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rate: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Upsampler").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self { rate },
        )
    }
}

#[async_trait]
impl AsyncKernel for Upsampler {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len() / self.rate);

        for j in 0..n {
            for k in 0..self.rate {
                o[j * self.rate + k] = i[j]
            }
        }

        println!("finished={} n={} i.len={}", sio.input(0).finished(), n, i.len());
        if sio.input(0).finished() && i.len() == n {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);
        sio.output(0).produce(n * self.rate);

        Ok(())
    }
}

pub struct Downsampler {
    rate: usize,
}

impl Downsampler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rate: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Downsampler").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self { rate },
        )
    }
}

#[async_trait]
impl AsyncKernel for Downsampler {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len() / self.rate, o.len());

        for j in 0..n {
            o[j] = 0.0;
            for k in 0..self.rate {
                o[j] += i[j * self.rate + k] / self.rate as f32;
            }
        }

        if sio.input(0).finished() && i.len() < self.rate {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n * self.rate);
        sio.output(0).produce(n);

        Ok(())
    }
}

const SYNC_LENGHT: usize = 40;
const SYNCA_SEQ: [bool; 40] = [
    false, false, false, false, // Start
    true, true, false, false, // Pulse 1
    true, true, false, false, // Pulse 2
    true, true, false, false, // Pulse 3
    true, true, false, false, // Pulse 4
    true, true, false, false, // Pulse 5
    true, true, false, false, // Pulse 6
    true, true, false, false, // Pulse 7
    false, false, false, false, // Tail part 1
    false, false, false, false, // Tail part 2
];
const SYNCB_SEQ: [bool; 40] = [
    false, false, false, false, // Start
    true, true, true, false, false, // Pulse 1
    true, true, true, false, false, // Pulse 2
    true, true, true, false, false, // Pulse 3
    true, true, true, false, false, // Pulse 4
    true, true, true, false, false, // Pulse 5
    true, true, true, false, false, // Pulse 6
    true, true, true, false, false, // Pulse 7
    false, // Tail
];

const PIXELS_PER_LINE: usize = 2080;
const LINES: usize = 500;

pub struct APTImageSink {
    avg_level: f32,
    state: [f32; SYNC_LENGHT],
    pos: usize,

    x: usize,
    y: usize,
    has_sync: bool,
    max_level: f32,
    previous_pixel: f32,
    img: image::GrayImage,
    path: String,
}

impl APTImageSink {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(path: String) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("APTImageSink").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                avg_level: 0.5,
                state: [0.0; SYNC_LENGHT],
                pos: 0,
                x: 0,
                y: 0,
                has_sync: false,
                max_level: 0.0,
                previous_pixel: 0.0,
                img: image::DynamicImage::new_luma8(PIXELS_PER_LINE as u32, LINES as u32)
                    .into_luma8(),
                path,
            },
        )
    }
}

impl APTImageSink {
    fn is_marker(&mut self) -> (bool, bool) {
        let mut count_a = 0;
        let mut count_b = 0;
        for i in 0..SYNC_LENGHT {
            let sync_pos = (self.pos + i) % SYNC_LENGHT;
            let sample = self.state[sync_pos] / (self.avg_level * 2.0);
            if (sample > 0.5 && SYNCA_SEQ[i]) || (sample <= 0.5 && !SYNCA_SEQ[i]) {
                count_a += 1;
            }
            if (sample > 0.5 && SYNCB_SEQ[i]) || (sample <= 0.5 && !SYNCB_SEQ[i]) {
                count_b += 1;
            }
        }
        return (count_a > 35, count_b > 35);
    }
}

#[async_trait]
impl AsyncKernel for APTImageSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();

        let n = i.len();

        for j in 0..n {
            let (is_a, is_b) = self.is_marker();

            let pixel = self.state[self.pos];

            self.state[self.pos] = i[j];
            self.avg_level = 0.25 * i[j] + self.avg_level * 0.75;
            self.pos = (self.pos + 1) % SYNC_LENGHT;

            if is_a {
                if !self.has_sync {
                    self.max_level = 0.0;
                    self.has_sync = true;
                }
                self.x = 0;
            } else if is_b {
                if self.x < (PIXELS_PER_LINE / 2) {
                    let skip_distance = (PIXELS_PER_LINE / 2) - self.x;
                    let color = (self.previous_pixel / self.max_level * 255.0) as u8;
                    for i in 0..skip_distance {
                        self.img.put_pixel(
                            (self.x + i) as u32,
                            self.y as u32,
                            image::Luma([color]),
                        );
                    }
                }

                if !self.has_sync {
                    self.max_level = 0.0;
                    self.has_sync = true;
                }
                self.x = PIXELS_PER_LINE / 2;
            }

            self.max_level = f32::max(pixel, self.max_level);
            let color = (pixel / self.max_level * 255.0) as u8;
            //println!("Color {} {}: {}", self.x, self.y, color);
            if self.y < LINES {
                self.img
                    .put_pixel(self.x as u32, self.y as u32, image::Luma([color]));
            }
            self.x += 1;
            if self.x >= PIXELS_PER_LINE {
                self.x = 0;
                self.y += 1;

                if self.y % 100 == 0 {
                    self.img.save(&self.path);
                }
            }
            self.previous_pixel = pixel;
        }

        if sio.input(0).finished() {
            io.finished = true;
            self.img.save(&self.path);
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);

        Ok(())
    }
}
