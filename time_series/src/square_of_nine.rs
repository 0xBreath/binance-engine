use log::debug;

#[derive(Clone, Copy, Debug)]
pub enum Harmonic {
  Zero = 0,
  OneEighth = 45,
  OneFourth = 90,
  ThreeEighths = 135,
  OneHalf = 180,
  FiveEighths = 225,
  ThreeFourths = 270,
  SevenEighths = 315,
}
impl Harmonic {
  fn next(&self) -> Self {
    // match self {
    //   Harmonic::Zero => Harmonic::SevenEighths,
    //   Harmonic::OneEighth => Harmonic::Zero,
    //   Harmonic::OneFourth => Harmonic::OneEighth,
    //   Harmonic::ThreeEighths => Harmonic::OneFourth,
    //   Harmonic::OneHalf => Harmonic::ThreeEighths,
    //   Harmonic::FiveEighths => Harmonic::OneHalf,
    //   Harmonic::ThreeFourths => Harmonic::FiveEighths,
    //   Harmonic::SevenEighths => Harmonic::ThreeFourths,
    // }
    match self {
      Harmonic::Zero => Harmonic::OneEighth,
      Harmonic::OneEighth => Harmonic::OneFourth,
      Harmonic::OneFourth => Harmonic::ThreeEighths,
      Harmonic::ThreeEighths => Harmonic::OneHalf,
      Harmonic::OneHalf => Harmonic::FiveEighths,
      Harmonic::FiveEighths => Harmonic::ThreeFourths,
      Harmonic::ThreeFourths => Harmonic::SevenEighths,
      Harmonic::SevenEighths => Harmonic::Zero,
    }
  }

  fn to_num(self) -> f32 {
    match self {
      Harmonic::Zero => 0.0,
      Harmonic::OneEighth => 45.0,
      Harmonic::OneFourth => 90.0,
      Harmonic::ThreeEighths => 135.0,
      Harmonic::OneHalf => 180.0,
      Harmonic::FiveEighths => 225.0,
      Harmonic::ThreeFourths => 270.0,
      Harmonic::SevenEighths => 315.0,
    }
  }
}

#[derive(Clone, Copy, Debug)]
pub struct Angle(pub f32);

impl Angle {
  pub fn new(angle: f32) -> Self {
    Angle(angle)
  }
  pub fn get(self) -> f32 {
    self.0
  }
  pub fn decrement(&mut self, value: f32) -> f32 {
    self.0 -= value;
    if self.0 >= 360.0 {
      self.0 -= 360.0;
    } else if self.0 < 0.0 {
      self.0 += 360.0;
    }
    self.0
  }
  pub fn increment(&mut self, value: f32) -> f32 {
    self.0 += value;
    if self.0 >= 360.0 {
      self.0 -= 360.0;
    } else if self.0 < 0.0 {
      self.0 += 360.0;
    }
    self.0
  }
  fn set(&mut self, angle: f32) {
    self.0 = angle;
  }
  /// Intended to function like array bounds, with range maximum being exclusive.
  fn to_arc(mut self, arc_range: f32) -> (f32, f32) {
    (self.0, self.increment(arc_range))
  }
}

#[derive(Clone, Copy, Debug)]
pub struct Point {
  pub value: f64,
  pub harmonic: Option<Harmonic>,
  pub arc: Option<(f32, f32)>
}
impl Default for Point {
  fn default() -> Self {
    Self {
      value: 0.0,
      harmonic: None,
      arc: None,
    }
  }
}

#[derive(Clone, Debug)]
pub struct SquareOfNine {
  /// Starting value of Square of Nine
  pub origin: f64,
  /// Matrix of coordinates that represent the 'Square of Nine'.
  /// Used for plotting numbers in a visual-friendly grid.
  pub matrix: Vec<Vec<Point>>,//[[Point; N]; N],
  /// One-dimensional vector of points in `Square of Nine`.
  /// Used for calculating 'Time=Price' alignments. Not intended for plotting.
  pub values: Vec<Point>,
  /// Delta between each point in `Square of Nine`.
  pub step: f64,
}

impl SquareOfNine {
  pub fn new(origin: u32, step: f64, dimension: u32) -> Self {
    let origin = origin as  f64;
    if dimension % 2 == 0 {
      panic!("matrix size N must be odd");
    }
    let mut matrix = Vec::<Vec<Point>>::new();
    for _ in 0..dimension {
      let mut row = Vec::<Point>::new();
      for _ in 0..dimension {
        row.push(Point::default());
      }
      matrix.push(row);
    }
    let mut values = Vec::<Point>::new();
    let mut value = origin;
    let center = dimension / 2;

    let mut x = center as usize;
    let mut y = center as usize;

    // origin
    let point = Point {
      value,
      harmonic: None,
      arc: None
    };
    matrix[y][x] = point;
    values.push(point);



    // index gap between harmonics. Increases as spiral grows.
    let mut empty_harmonics = 0;
    // each new harmonic rotates/increments harmonic.
    // Starting harmonic is OneHalf and rotates counter-clockwise
    let mut harmonic = Harmonic::Zero;
    let mut angle = Angle::new(harmonic.to_num());
    // loop through matrix size
    // each index is used twice as the matrix spirals
    let mut index = 1;
    // size of one spiral of matrix. Increases by 8 with each new spiral.
    let mut spiral_size = 8;
    while index < dimension {
      let arc_per_integer = 360.0 / spiral_size as f32;
      // shift left by 1 to start next spiral
      x -= 1;
      value += step;
      if index == 1 {
        angle.set(harmonic.to_num());
        let point = Point {
          value,
          harmonic: Some(harmonic),
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
      } else {
        angle.set(harmonic.to_num() - arc_per_integer);
        let point = Point {
          value,
          harmonic: None,
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
      }

      // spiral up by index
      let mut inner_index = 0;
      let mut up_empty_harmonics_length = empty_harmonics - 1;
      while inner_index < index {
        for _ in 0..up_empty_harmonics_length {
          y -= 1;
          value += step;
          angle.decrement(arc_per_integer);
          let point = Point {
            value,
            harmonic: None,
            arc: Some(angle.to_arc(arc_per_integer))
          };
          matrix[y][x] = point;
          values.push(point);
          inner_index += 1;
        }

        y -= 1;
        value += step;
        harmonic = harmonic.next();
        angle.set(harmonic.to_num());
        let point = Point {
          value,
          harmonic: Some(harmonic),
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
        inner_index += 1;
        up_empty_harmonics_length += 1;
      }
      inner_index = 0;

      // spiral right by index + 1
      while inner_index < index + 1 {
        for _ in 0..empty_harmonics {
          x += 1;
          value += step;
          angle.decrement(arc_per_integer);
          let point = Point {
            value,
            harmonic: None,
            arc: Some(angle.to_arc(arc_per_integer))
          };
          matrix[y][x] = point;
          values.push(point);
          inner_index += 1;
        }

        x += 1;
        value += step;
        harmonic = harmonic.next();
        angle.set(harmonic.to_num());
        let point = Point {
          value,
          harmonic: Some(harmonic),
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
        inner_index += 1;
      }
      inner_index = 0;

      // spiral down by index + 1
      while inner_index < index + 1 {
        for _ in 0..empty_harmonics {
          y += 1;
          value += step;
          angle.decrement(arc_per_integer);
          let point = Point {
            value,
            harmonic: None,
            arc: Some(angle.to_arc(arc_per_integer))
          };
          matrix[y][x] = point;
          values.push(point);
          inner_index += 1;
        }

        y += 1;
        value += step;
        harmonic = harmonic.next();
        angle.set(harmonic.to_num());
        let point = Point {
          value,
          harmonic: Some(harmonic),
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
        inner_index += 1;
      }
      inner_index = 0;

      while inner_index < index + 1 {
        for _ in 0..empty_harmonics {
          x -= 1;
          value += step;
          angle.decrement(arc_per_integer);
          let point = Point {
            value,
            harmonic: None,
            arc: Some(angle.to_arc(arc_per_integer))
          };
          matrix[y][x] = point;
          values.push(point);
          inner_index += 1;
        }

        x -= 1;
        value += step;
        harmonic = harmonic.next();
        angle.set(harmonic.to_num());
        let point = Point {
          value,
          harmonic: Some(harmonic),
          arc: Some(angle.to_arc(arc_per_integer))
        };
        matrix[y][x] = point;
        values.push(point);
        inner_index += 1;
      }

      index += 2;
      empty_harmonics += 1;
      spiral_size += 8;
    }

    Self {
      origin,
      matrix,
      values,
      step
    }
  }
  
  pub fn get_step(&self) -> f64 {
    self.step
  }

  pub fn get_point(&self, x: usize, y: usize) -> Option<&Point> {
    self.matrix.get(y).and_then(|row| row.get(x))
  }

  pub fn get_values(&self) -> &[Point] {
    &self.values
  }

  /// Search SquareOfNine for all prices (harmonic points) along longitude.
  /// Price=Time
  pub fn find_price_equals_time(&self, angle: f32) -> Vec<Point> {
    let mut points = Vec::new();
    for point in self.values.iter() {
      if let Some((range_min, range_max)) = point.arc {
        if range_min < range_max {
          if angle >= range_min && angle < range_max {
            points.push(*point);
          }
        } else if (range_min > range_max && angle >= range_min && angle < 360.0) || (range_min > range_max && angle < range_max && angle >= 0.0) {
          points.push(*point);
        }
      }
    }
    points
  }

  pub fn test_square_of_nine(dimension: u32) {
    if dimension % 2 == 0 {
      panic!("Dimension must be odd");
    }
    let square_of_nine = SquareOfNine::new(1, 1.0, dimension);
    if dimension < 13 {
      for y in square_of_nine.matrix.iter() {
        for x in y {
          match x.harmonic {
            Some(harmonic) => print!("{}\t", harmonic as i32),
            None => print!("-\t"),
          }
        }
        println!();
      }
      println!("--------------------------------------------------------------------------------------------------------");
      for y in square_of_nine.matrix.iter() {
        for x in y {
          print!("{}\t", x.value)
        }
        println!();
      }
      println!("--------------------------------------------------------------------------------------------------------");
    }
    //used to check size of outermost square of nine ring
    let zero_zero = square_of_nine.matrix[0][0].value;
    let one_one = square_of_nine.matrix[1][1].value;
    let two_two = square_of_nine.matrix[2][2].value;
    println!("Size of outermost ring: {:?}", (zero_zero - one_one) as u32);
    println!("Size of second outermost ring: {:?}", (one_one - two_two) as u32);
    println!("--------------------------------------------------------------------------------------------------------");
    println!("PRICE\tHARMONIC\t\tDEGREES OF ARC");
    for point in square_of_nine.values.iter() {
      match point.harmonic {
        None => {
          match point.arc {
            None => println!("{:?}\t{:?}\t\t\t{}\r", point.value, point.harmonic, "-"),
            Some(arc) => println!("{:?}\t{:?}\t\t\t{:?}\r", point.value, point.harmonic, arc),
          }
        },
        Some(Harmonic::Zero) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::OneEighth) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::OneFourth) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::ThreeEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::OneHalf) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::FiveEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::ThreeFourths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
        Some(Harmonic::SevenEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
      }
    }
    // println!("--------------------------------------------------------------------------------------------------------");
    // let harmonics_zero = square_of_nine.find_price_equals_time(0.0);
    // println!("ZERO HARMONICS");
    // for point in harmonics_zero.iter() {
    //   match point.harmonic {
    //     None => {
    //       match point.arc {
    //         None => println!("{:?}\t{:?}\t\t\t{}\r", point.value, point.harmonic, "-"),
    //         Some(arc) => println!("{:?}\t{:?}\t\t\t{:?}\r", point.value, point.harmonic, arc),
    //       }
    //     },
    //     Some(Harmonic::Zero) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::OneEighth) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::OneFourth) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::ThreeEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::OneHalf) => println!("{:?}\t{:?}\t\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::FiveEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::ThreeFourths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //     Some(Harmonic::SevenEighths) => println!("{:?}\t{:?}\t{:?}\r", point.value, point.harmonic, point.arc.unwrap()),
    //   }
    // }
  }

  /// Check if value is within margin of error of a target.
  pub fn within_margin_of_error(target: f64, value: f64, margin_of_error: f64) -> bool {
    let margin = target * margin_of_error;
    value >= target - margin && target + margin >= value
  }

  /// Search the SquareOfNine for a cell value.
  /// Return the number of cells in the ring that contains the cell value
  /// Used to determines cell value range for a 360 degree rotation.
  pub fn ring_size_of_cell_value(&self, value: f64) -> Option<u64> {
    let mut ring_size = None;
    let mut angle = 0.0;
    let mut point_index = 0;
    let mut point_value = 0.0;
    let mut found = false;

    for (index, point) in self.values.iter().enumerate() {
      if SquareOfNine::within_margin_of_error(value, point.value, 0.01) {
        match point.arc {
          None => ring_size = None,
          Some(arc) => {
            if arc.1 > arc.0 {
              let median = (arc.1 - arc.0) / 2.0;
              angle = Angle::new(arc.1).decrement(median);
            } else {
              let median = (arc.0 - arc.1) / 2.0;
              angle = Angle::new(arc.0).decrement(median);
            }
            point_index = index;
            point_value = point.value;
          }
        }
        found = true;
        break;
      }
    }
    if found {
      for index in (point_index + 1)..self.values.len() {
        let point = self.values.get(index).expect("Point not found");
        if let Some(arc) = point.arc {
          if arc.0 < arc.1 {
            if angle >= arc.0 && angle < arc.1 {
              debug!("angle: {:?}, arc: {:?}", angle, arc);
              ring_size = Some((point.value - point_value) as u64);
              break;
            }
          } else if (arc.0 > arc.1 && angle >= arc.0 && angle < 360.0) || (arc.0 > arc.1 && angle < arc.1 && angle >= 0.0) {
            debug!("angle: {:?}, arc: {:?}", angle, arc);
            ring_size = Some((point.value - point_value) as u64);
            break;
          }
        }
      }
    }
    ring_size
  }
}