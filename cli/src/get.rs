#[derive(structopt::StructOpt)]
struct Args {
    /// Input bias
    #[structopt(short, long, default_value = "disable")]
    bias: gpiod::Bias,

    /// Active state
    #[structopt(short, long, default_value = "high")]
    active: gpiod::Active,

    /// Consumer string
    #[structopt(short, long, default_value = "gpioget")]
    consumer: String,

    /// GPIO chip
    #[structopt()]
    chip: std::path::PathBuf,

    /// GPIO lines
    #[structopt()]
    lines: Vec<gpiod::LineId>,
}

#[paw::main]
fn main(args: Args) -> anyhow::Result<()> {
    if args.lines.len() > gpiod::MAX_VALUES {
        anyhow::bail!("Too many lines");
    }

    let chip = gpiod::Chip::new(&args.chip)?;

    let input = chip.request_lines(
        gpiod::Options::input(&args.lines)
            .active(args.active)
            .bias(args.bias)
            .consumer(&args.consumer),
    )?;

    let values = args.lines.iter().map(|_| false).collect::<Vec<_>>();

    let values = input.get_values(values)?;

    for value in values {
        print!("{} ", if value { 1 } else { 0 });
    }
    println!("");

    Ok(())
}
