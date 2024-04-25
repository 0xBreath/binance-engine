use plotters::prelude::*;
use plotters::style::full_palette::*;
use plotters::style::{BLACK, WHITE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data {
  pub x: i64,
  pub y: f64,
}

pub struct Plot;

impl Plot {
  pub fn plot(series: Vec<Vec<Data>>, out_file: &str, title: &str, y_label: &str) -> anyhow::Result<()> {

    let all: Vec<&Data> = series.iter().flatten().collect();

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for datum in all.iter() {
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

    let root = BitMapBackend::new(out_file, (2048, 1024)).into_drawing_area();
    root.fill(&WHITE).map_err(
      |e| anyhow::anyhow!("Failed to fill drawing area with white: {}", e)
    )?;
    let mut chart = ChartBuilder::on(&root)
      .margin_top(20)
      .margin_bottom(20)
      .margin_left(30)
      .margin_right(30)
      .set_all_label_area_size(140)
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
    
    let colors = vec![
      CYAN_800,
      RED_800,
      LIME_800,
      PURPLE_200,
      ORANGE_A200,
      BLUE_800,
      GREY_900,
      BROWN_700,
    ];

    for (index, data) in series.into_iter().enumerate() {
      let color = RGBAColor::from(colors[index]);
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
    }

    root.present().map_err(
      |e| anyhow::anyhow!("Failed to present root: {}", e)
    )?;

    Ok(())
  }

  pub fn random_color() -> RGBAColor {
    let colors = [
      PINK_600,
      RED_800,
      DEEPORANGE_200,
      YELLOW_600,
      ORANGE_A200,
      AMBER_800,
      LIME_800,
      CYAN_800,
      BLUE_800,
      DEEPPURPLE_A100,
      PURPLE_200,
      GREY_900,
      BROWN_700,
    ];
    // get random color
    RGBAColor::from(colors[rand::random::<usize>() % colors.len()])
  }
}