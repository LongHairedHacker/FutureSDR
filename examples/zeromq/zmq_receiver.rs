use futuresdr::anyhow::Result;
use futuresdr::blocks::zeromq::SubSourceBuilder;
use futuresdr::blocks::FileSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let zmq_src = fg.add_block(
        SubSourceBuilder::new(1)
            .address("tcp://127.0.0.1:50001")
            .build(),
    );
    let snk = fg.add_block(FileSink::new(1, "/tmp/zmq-log.bin"));

    fg.connect_stream(zmq_src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
