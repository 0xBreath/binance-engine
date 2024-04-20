use lib::DreamrunnerResult;
use lib::trade::Data;
use plotters::prelude::*;
use plotters::prelude::full_palette::{PURPLE_400};

pub struct Plot;

impl Plot {
  pub fn plot(data: Vec<Data>, file_name: &str, title: &str, y_label: &str) -> DreamrunnerResult<()> {

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for datum in data.iter() {
      if datum.x < min_x {
        min_x = datum.x;
      }
      if datum.x > max_x {
        max_x = datum.x;
      }
      if datum.y < min_y {
        min_y = datum.y;
      }
      if datum.y > max_y {
        max_y = datum.y;
      }
    }

    let out_file = &format!("{}/{}.png", env!("CARGO_MANIFEST_DIR"), file_name);
    let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
    root.fill(&WHITE).map_err(
      |e| anyhow::anyhow!("Failed to fill drawing area with white: {}", e)
    )?;
    let mut chart = ChartBuilder::on(&root)
      .margin_top(20)
      .margin_bottom(20)
      .margin_left(30)
      .margin_right(30)
      .set_all_label_area_size(100)
      .caption(
        title,
        ("sans-serif", 40.0).into_font(),
      )
      .build_cartesian_2d(min_x..max_x, min_y..max_y).map_err(
      |e| anyhow::anyhow!("Failed to build cartesian 2d: {}", e)
    )?;
    chart
      .configure_mesh()
      .light_line_style(WHITE)
      .label_style(("sans-serif", 30, &BLACK).into_text_style(&root))
      .x_desc("UNIX Milliseconds")
      .y_desc(y_label)
      .draw().map_err(
      |e| anyhow::anyhow!("Failed to draw mesh: {}", e)
    )?;

    // get color from colors array
    // DEEPPURPLE_A11
    let color = RGBAColor::from(PURPLE_400);

    chart.draw_series(
      LineSeries::new(
        data.iter().map(|data| (data.x, data.y)),
        ShapeStyle {
          color,
          filled: true,
          stroke_width: 2,
        },
      )
        .point_size(3),
    ).map_err(
      |e| anyhow::anyhow!("Failed to draw series: {}", e)
    )?;

    root.present().map_err(
      |e| anyhow::anyhow!("Failed to present root: {}", e)
    )?;
    
    Ok(())
  }
}