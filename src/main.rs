#![feature(exclusive_range_pattern)]

use byteorder::{BigEndian, WriteBytesExt};
use bytes::buf::BufExt as _;
use chrono::Local;
use embedded_graphics::{
    drawable::Pixel,
    image::Image1BPP,
    prelude::{UnsignedCoord, *},
    Drawing,
};
use hyper::client::Client;
use hyper_tls::HttpsConnector;
use profont::{ProFont14Point, ProFont24Point, ProFont9Point};
use serde_derive::{Deserialize, Serialize};
use serialport::open;
use std::{io::prelude::*, str};
use dotenv_codegen::dotenv;
use textwrap::fill;

pub const ROWS: u16 = 128;
pub const COLS: u16 = 250;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Color {
    Black,
    White,
}

impl PixelColor for Color {}

impl From<u8> for Color {
    fn from(value: u8) -> Self {
        match value {
            0 => Color::Black,
            1 => Color::White,
            _ => panic!("invalid color value"),
        }
    }
}

impl From<u16> for Color {
    fn from(value: u16) -> Self {
        match value {
            0 => Color::Black,
            1 => Color::White,
            _ => panic!("invalid color value"),
        }
    }
}

struct Display<'a> {
    buff: &'a mut [u8],
}

impl<'a> Display<'a> {
    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        let (index, bit) = get_bit(x, y, ROWS as u32, COLS as u32);
        let index = index as usize;

        match color {
            Color::Black => {
                self.buff[index] &= !bit;
            }
            Color::White => {
                self.buff[index] |= bit;
            }
        }
    }
}

impl<'a> Drawing<Color> for Display<'a> {
    fn draw<T>(&mut self, item_pixels: T)
    where
        T: IntoIterator<Item = Pixel<Color>>,
    {
        for Pixel(UnsignedCoord(x, y), colour) in item_pixels {
            self.set_pixel(x, y, colour);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut serialport = open("/dev/ttyACM2").expect("unable to open serial port");

    let token = dotenv!("API_KEY");
    let lat: f32 = 31.1171;
    let long: f32 = -97.7278;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri = format!(
        "https://api.darksky.net/forecast/{}/{},{}",
        token, lat, long
    )
    .parse()?;
    let resp = client.get(uri).await?;
    println!("Response: {}", resp.status());

    let body = hyper::body::aggregate(resp).await?;

    // try to parse as json with serde_json
    let forecast: Forecast = serde_json::from_reader(body.reader())?;

    println!("{:#?}", forecast);

    let mut buf = [255u8; ROWS as usize * COLS as usize / 8];
    let mut display = Display { buff: &mut buf };
    let now = Local::now();

    let formatted = now.format("%H:%M").to_string();
    let t = ProFont24Point::render_str(&formatted)
        .stroke(Some(Color::Black))
        .fill(Some(Color::White))
        .translate(Coord::new(0, 20));
    display.draw(t);

    let formatted = now.format("%d/%m/%y").to_string();
    let t = ProFont14Point::render_str(&formatted)
        .stroke(Some(Color::Black))
        .fill(Some(Color::White))
        .translate(Coord::new(0, 0));
    display.draw(t);

    if let Some(currently) = forecast.currently {
        if let Some(temp) = currently.temperature {
            let temp = format!("{:2.0}°", temp);
            let t = ProFont14Point::render_str(&temp)
                .stroke(Some(Color::Black))
                .fill(Some(Color::White))
                .translate(Coord::new(130, 0));
            display.draw(t);
        }
        if let Some(precip) = currently.precip_probability {
            let precip = format!("{:2.0}%", precip);
            let t = ProFont14Point::render_str(&precip)
                .stroke(Some(Color::Black))
                .fill(Some(Color::White))
                .translate(Coord::new(162, 0));
            display.draw(t);
        }
        if let Some(wind) = currently.wind_speed {
            if let Some(dir) = currently.wind_bearing {
                // to convert image
                // convert src/clear.bmp -depth 1 -alpha off -fill white -resize 40x40 gray:"src/clear256.bmp"
                let mut image = match dir as i32 {
                    337..360 | 0..22 => Image1BPP::new(include_bytes!("arrow_north.bmp"), 40, 40), //north
                    22..67 => Image1BPP::new(include_bytes!("arrow_north_east.bmp"), 40, 40), //north_east
                    67..112 => Image1BPP::new(include_bytes!("arrow_east.bmp"), 40, 40),      //east
                    112..157 => Image1BPP::new(include_bytes!("arrow_south_east.bmp"), 40, 40), //south_east
                    157..202 => Image1BPP::new(include_bytes!("arrow_south.bmp"), 40, 40), //south
                    202..247 => Image1BPP::new(include_bytes!("arrow_south_west.bmp"), 40, 40), //south_west
                    247..292 => Image1BPP::new(include_bytes!("arrow_west.bmp"), 40, 40), //west
                    292..337 => Image1BPP::new(include_bytes!("arrow_north_west.bmp"), 40, 40), //northwest
                    _ => Image1BPP::new(include_bytes!("arrow_north.bmp"), 40, 40),
                };

                image.translate_mut(Coord::new(86, 44));
                display.draw(&image);
            }
            let wind = format!("{:2.0}MPH", wind);
            let t = ProFont14Point::render_str(&wind)
                .stroke(Some(Color::Black))
                .fill(Some(Color::White))
                .translate(Coord::new(200, 0));
            display.draw(t);
        }
        let mut count = 0;
        if let Some(summary) = currently.summary {
            let summary = format!("Currently: {}", summary);
            let text = fill(&summary, 20);
            for (i, line) in text.split('\n').enumerate() {
                count += i as i32;
                let t = ProFont9Point::render_str(&line)
                    .stroke(Some(Color::Black))
                    .fill(Some(Color::White))
                    .translate(Coord::new(130, 20 + (count * 10)));
                display.draw(t);
            }
        }
        count += 1;
        if let Some(daily) = forecast.daily {
            if let Some(data) = daily.data {
                if let Some(summary) = &data[0].summary {
                    let summary = format!("Today: {}", summary);
                    let text = fill(&summary, 20);
                    for (i, line) in text.split('\n').enumerate() {
                        let t = ProFont9Point::render_str(&line)
                            .stroke(Some(Color::Black))
                            .fill(Some(Color::White))
                            .translate(Coord::new(130, 20 + ((i as i32 + count) * 10)));
                        display.draw(t);
                    }
                }
            }
        }
        match currently.icon {
            Some(Icon::ClearDay) => {
                let mut image = Image1BPP::new(include_bytes!("clearday.bmp"), 40, 40);
                image.translate_mut(Coord::new(86, 0));
                display.draw(&image);
            }
            Some(Icon::ClearNight) => {
                let mut image = Image1BPP::new(include_bytes!("clearnight.bmp"), 40, 40);
                image.translate_mut(Coord::new(86, 0));
                display.draw(&image);
            }
            Some(Icon::PartlyCloudyDay) => {
                let mut image = Image1BPP::new(include_bytes!("partlycloudyday.bmp"), 40, 40);
                image.translate_mut(Coord::new(86, 0));
                display.draw(&image);
            }
            _ => {}
        }
    }

    let mut buff = Vec::new();
    buff.write_u32::<BigEndian>(display.buff.len() as u32)
        .unwrap();
    serialport.write(&buff).unwrap();
    serialport.write(display.buff).unwrap();
    Ok(())
}

fn get_bit(x: u32, y: u32, width: u32, height: u32) -> (u32, u8) {
    (y / 8 + (height - 1 - x) * (width / 8), 0x80 >> (y % 8))
}

#[derive(Deserialize, Debug)]
struct Forecast {
    latitude: f32,
    longitude: f32,
    timezone: String,
    currently: Option<Datapoint>,
    pub daily: Option<Datablock>,
    pub hourly: Option<Datablock>,
    pub minutely: Option<Datablock>,
    pub flags: Option<Flags>,
}

/// A datapoint within a [`Datablock`], where there is usually multiple.
///
/// All fields are optional _except for [`time`]_, as some data may not be
/// available for a location at a given point in time.
///
/// All of the data oriented fields may have associated `error` fields,
/// representing the confidence in a prediction or value. An example is
/// [`precip_accumulation`], which has an associated error field of
/// [`precip_accumulation_error`]. Those fields represent standard deviations of
/// the value of the associated field. Smaller error values represent greater
/// confidence levels, while larger error values represent less confidence.
/// These fields are omitted where the confidence is not precisely known.
///
/// [`Datablock`]: struct.Datablock.html
/// [`time`]: #structfield.time
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Datapoint {
    /// The unix timestamp representing when the daytime high apparent
    /// temperature occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub apparent_temperature_max_time: Option<u64>,
    /// The daytime high apparent temperature.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub apparent_temperature_max: Option<f64>,
    /// The unix timestamp representing when the overnight low apparent
    /// temperature occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub apparent_temperature_min_time: Option<u64>,
    /// The overnight low apparent temperature.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub apparent_temperature_min: Option<f64>,
    /// The apparent (or "feels like") temperature in degrees Fahrenheit.
    ///
    /// **Note**: This is not present on `daily`.
    pub apparent_temperature: Option<f64>,
    /// The amount of error possible within the [`cloud_cover`] value.
    ///
    /// [`cloud_cover`]: #structfield.cloud_cover
    pub cloud_cover_error: Option<f64>,
    /// The percentage of sky occluded by clouds.
    ///
    /// This value is between `0` and `1`, inclusively.
    pub cloud_cover: Option<f64>,
    /// The amount of error possible within the [`dew_point`] value.
    ///
    /// [`dew_point`]: #structfield.dew_point
    pub dew_point_error: Option<f64>,
    /// The dew point in degrees Fahrenheit.
    pub dew_point: Option<f64>,
    /// The amount of error possible within the [`humidity`] value.
    ///
    /// [`humidity`]: #structfield.humidity
    pub humidity_error: Option<f64>,
    /// The relative humidity.
    ///
    /// This value is between `0` and `1`, inclusively.
    pub humidity: Option<f64>,
    /// A machine-readable summary of the datapoint, suitable for selecting an
    /// icon to display.
    pub icon: Option<Icon>,
    /// The fractional part of the [lunation number] during the given day.
    ///
    /// A value of `0` corresponds to a new moon, `0.25` to a first quarter
    /// moon, `0.5` to a full moon, `0.75` to a last quarter moon.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub moon_phase: Option<f64>,
    /// The approximate direction of the nearest storm in degrees, with true
    /// north at 0 degrees and progressing clockwise.
    ///
    /// If `nearestStormDistance` is `0`, then this value will not be present.
    ///
    /// **Note**: This is only present on the `currently` block.
    pub nearest_storm_bearing: Option<f64>,
    /// The approximate distance to the nearest storm in miles.
    ///
    /// A storm distance of `0` doesn't necessarily refer to a storm at the
    /// requested location, but rather a storm in the vicinity of that location.
    ///
    /// **Note**: This is only present on the `currently` block.
    pub nearest_storm_distance: Option<f64>,
    /// The amount of error possible within the [`ozone`] value.
    ///
    /// [`ozone`]: #structfield.ozone
    pub ozone_error: Option<f64>,
    /// The columnar density of total atmospheric ozone at the given time in
    /// Dobson units.
    pub ozone: Option<f64>,
    /// The amount of error possible within the [`precip_accumulation`] value.
    ///
    /// [`precip_accumulation`]: #structfield.precip_accumulation
    pub precip_accumulation_error: Option<f64>,
    /// The amount of snowfall accumulation expected to occur, in inches.
    ///
    /// If no snowfall is expected, this will be None.
    ///
    /// **Note**: This is only present on `hourly` and `daily` blocks.
    pub precip_accumulation: Option<f64>,
    /// The amount of error possible within the [`precip_intensity`] value.
    ///
    /// [`precip_intensity`]: #structfield.precip_intensity
    pub precip_intensity_error: Option<f64>,
    /// The amount of error possible within the [`precip_intensity_max`] value.
    ///
    /// [`precip_intensity_max`]: #structfield.precip_intensity_max
    pub precip_intensity_max_error: Option<f64>,
    /// The unix timestamp of when [`precip_intensity_max`] occurs during a
    /// given day.
    ///
    /// **Note**: This is only present on the `daily` block.
    ///
    /// [`precip_intensity_max`]: #structfield.precip_intensity_max
    pub precip_intensity_max_time: Option<u64>,
    /// The maximum value of [`precip_intensity`] during a given day.
    ///
    /// **Note**: This is only present on the `daily` block.
    ///
    /// [`precip_intensity`]: #structfield.precip_intensity
    pub precip_intensity_max: Option<f64>,
    /// The intensity (in inches of liquid water per hour) precipitation
    /// occurring at the given time.
    ///
    /// This value is conditional on probability (that is, assuming any
    /// precipitation occurs at all) for `minutely` datapoints, and
    /// unconditional otherwise.
    pub precip_intensity: Option<f64>,
    /// The amount of error possible within the [`precip_probability`] value.
    ///
    /// [`precip_probability`]: #structfield.precip_probability
    pub precip_probability_error: Option<f64>,
    /// The probably of precipitation occurring.
    ///
    /// This value is between `0` and `1`, inclusively.
    pub precip_probability: Option<f64>,
    /// The type of precipitation occurring at a given time.
    ///
    /// If [`precip_intensity`] is `0`, then this field will be `None`.
    ///
    /// Additionally, due to the lack of data in DarkSky sources, historical
    /// `precip_type` values is usually estimated, rather than observed.
    ///
    /// [`precip_intensity`]: #structfield.precip_intensity
    pub precip_type: Option<PrecipitationType>,
    /// The amount of error possible within the [`pressure`] value.
    ///
    /// [`pressure`]: #structfield.pressure
    pub pressure_error: Option<f64>,
    /// The sea-level air pressure in millibars.
    pub pressure: Option<f64>,
    /// A human-readable text summary of the datapoint.
    ///
    /// **Note**: Do not use this for automated icon display purposes, use the
    /// [`icon`] field instead.
    ///
    /// [`icon`]: #structfield.icon
    pub summary: Option<String>,
    /// The unix timestamp of when the sun will rise during a given day.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub sunrise_time: Option<u64>,
    /// The unix timestamp of when the sun will set during a given day.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub sunset_time: Option<u64>,
    /// The overnight low temperature.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_low: Option<f64>,
    /// The unix timestamp representing when the overnight low temperature
    /// occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_low_time: Option<u64>,
    /// The daytime high temperature.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_high: Option<f64>,
    /// The unix timestamp representing when the daytime high temperature
    /// occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_high_time: Option<u64>,
    /// The amount of error possible within the [`temperature_max`] value.
    ///
    /// [`temperature_max`]: #structfield.temperature_max
    pub temperature_max_error: Option<f64>,
    /// The unix timestamp representing when the maximum temperature during a
    /// given date occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_max_time: Option<u64>,
    /// The maximum temperature during a given date.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_max: Option<f64>,
    /// The amount of error possible within the [`temperature_min`] value.
    ///
    /// [`temperature_min`]: #structfield.temperature_min
    pub temperature_min_error: Option<f64>,
    /// The unix timestamp representing when the minimum temperature during a
    /// given date occurs.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_min_time: Option<u64>,
    /// The minimum temperature during a given date.
    ///
    /// **Note**: This is only present on the `daily` block.
    pub temperature_min: Option<f64>,
    /// The amount of error possible within the [`temperature`] value.
    ///
    /// [`temperature`]: #structfield.temperature
    pub temperature_error: Option<f64>,
    /// The air temperature in degrees Fahrenheit.
    pub temperature: Option<f64>,
    /// The unix timestamp at which the datapoint begins.
    ///
    /// `minutely` datapoints are always aligned to the top of the minute.
    ///
    /// `hourly` datapoints align to the top of the hour.
    ///
    /// `daily` datapoints align to midnight of the day.
    ///
    /// All are according to the local timezone.
    pub time: u64,
    /// The UV index.
    pub uv_index: Option<u64>,
    /// The unix timestamp of when the maximum [`uv_index`] occurs during the
    /// given day.
    ///
    /// [`uv_index`]: #structfield.uv_index
    pub uv_index_time: Option<u64>,
    /// The amount of error possible within the [`visibility`] value.
    ///
    /// [`visibility`]: #structfield.visibility
    pub visibility_error: Option<f64>,
    /// The average visibility in miles, capped at 10 miles.
    pub visibility: Option<f64>,
    /// The amount of error possible within the [`wind_bearing`] value.
    ///
    /// [`wind_bearing`]: #structfield.wind_bearing
    pub wind_bearing_error: Option<f64>,
    /// The direction that the wind is coming from in degrees.
    ///
    /// True north is at 0 degrees, progressing clockwise.
    ///
    /// If [`wind_speed`] is `0`, then this value will not be defined.
    ///
    /// [`wind_speed`]: #structfield.wind_speed
    pub wind_bearing: Option<f64>,
    /// The wind gust speed in miles per hour.
    pub wind_gust: Option<f64>,
    /// The amount of time that the wind gust is expected to occur.
    pub wind_gust_time: Option<u64>,
    /// The amount of error possible within the [`wind_speed`] value.
    ///
    /// [`wind_speed`]: #structfield.wind_speed
    pub wind_speed_error: Option<f64>,
    /// The wind speed in miles per hour.
    pub wind_speed: Option<f64>,
}

/// The type of precipitation that is happening within a [`Datapoint`].
///
/// [`Datapoint`]: struct.Datapoint.html
#[derive(Copy, Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize)]
pub enum PrecipitationType {
    /// Indicator that the type of precipitation is rain.
    #[serde(rename = "rain")]
    Rain,
    /// Indicator that the type of precipitation is sleet.
    #[serde(rename = "sleet")]
    Sleet,
    /// Indicator that the type of precipitation is snow.
    #[serde(rename = "snow")]
    Snow,
}

/// A safe representation of the indicated weather. This is useful for matching
/// and presenting an emoji or other weather symbol or representation.
#[derive(Copy, Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize)]
pub enum Icon {
    /// The day's sky is clear.
    #[serde(rename = "clear-day")]
    ClearDay,
    /// The night sky is clear.
    #[serde(rename = "clear-night")]
    ClearNight,
    /// The sky is cloudy.
    #[serde(rename = "cloudy")]
    Cloudy,
    /// It is foggy.
    #[serde(rename = "fog")]
    Fog,
    /// Not actively in use
    #[serde(rename = "hail")]
    Hail,
    /// The day's sky is partly cloudy.
    #[serde(rename = "partly-cloudy-day")]
    PartlyCloudyDay,
    /// The night's sky is partly night.
    #[serde(rename = "partly-cloudy-night")]
    PartlyCloudyNight,
    /// The weather is rain.
    #[serde(rename = "rain")]
    Rain,
    /// The weather is sleet.
    #[serde(rename = "sleet")]
    Sleet,
    /// The weather is snow.
    #[serde(rename = "snow")]
    Snow,
    /// Not actively in use
    #[serde(rename = "thunderstorm")]
    Thunderstorm,
    /// Not actively in use
    #[serde(rename = "tornado")]
    Tornado,
    /// The weather is windy.
    #[serde(rename = "wind")]
    Wind,
}

/// A set of flags for a forecast, such as the [`Unit`]s specified or the vector
/// of [DarkSky] stations reporting.
///
/// [`Unit`]: enum.Unit.html
/// [DarkSky]: https://darksky.net
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Flags {
    /// A list of DarkSky stations used for the [`Forecast`].
    ///
    /// [`Forecast`]: struct.Forecast.html
    pub darksky_stations: Option<Vec<String>>,
    /// A list of the unavailable DarkSky stations.
    pub darksky_unavailable: Option<String>,
    /// A list of the
    pub datapoint_stations: Option<Vec<String>>,
    /// A list of [ISD] stations used.
    ///
    /// [ISD]: https://www.ncdc.noaa.gov/isd
    pub isd_stations: Option<Vec<String>>,
    /// A list of [LAMP] stations used to obtain the information.
    ///
    /// [LAMP]: http://www.nws.noaa.gov/mdl/lamp/lamp_info.shtml
    pub lamp_stations: Option<Vec<String>>,
    /// A list of [METAR] stations used to obtain the information.
    ///
    /// [METAR]: https://www.aviationweather.gov/metar
    pub metar_stations: Option<Vec<String>>,
    /// The [METNO license] used.
    ///
    /// [METNO license]: http://www.met.no/
    pub metno_license: Option<String>,
    /// A list of sources used to obtain the information.
    pub sources: Option<Vec<String>>,
    /// The [`Unit`]s used to format the data.
    ///
    /// [`Unit`]: enum.Unit.html
    pub units: Option<String>,
}

/// A block of data within a [`Forecast`], with potentially many [`Datapoint`]s.
///
/// [`Datapoint`]: struct.Datapoint.html
/// [`Forecast`]: struct.Forecast.html
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Datablock {
    /// The data for the datablock, if there is any data available.
    pub data: Option<Vec<Datapoint>>,
    /// The icon representing the weather type for the datablock.
    pub icon: Option<Icon>,
    /// A written summary of the datablock's expected weather.
    pub summary: Option<String>,
}
