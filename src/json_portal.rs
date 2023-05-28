use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;
use std::ops::{Add, Mul};
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Deserializer};
use serde_aux::field_attributes::{
    deserialize_number_from_string, deserialize_option_number_from_string,
};

use crate::Offer;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductOffers {
    #[serde(rename = "items")]
    products: Vec<Product>,
}

impl ProductOffers {
    pub fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        let filename = file.as_ref();
        serde_json::from_reader(std::fs::File::open(filename)?)
            .with_context(|| format!("Failed to parse json file {}", filename.display()))
    }

    pub fn get_internet_offers(&self) -> Vec<Offer> {
        let price_descr: Regex =
            Regex::new(r"(\d+) kr i (\d+) månader, därefter ordinarie pris (\d+) kr").unwrap();
        let mut offers = vec![];
        for p in self
            .products
            .iter()
            .filter(|p| p.categories.contains(&ProductType::Internet))
        {
            let (list_price, discounted_price, discount_duration) = if p.is_campaign {
                let x = price_descr
                    .captures(p.price_comment.as_ref().unwrap())
                    .expect(&*format!(
                        "Price regexp failed to match {}",
                        p.price_comment.as_ref().unwrap()
                    ));
                (
                    x[3].parse().unwrap(),
                    x[1].parse::<Sek>().unwrap(),
                    x[2].parse().unwrap(),
                )
            } else {
                (p.price_per_month, Sek(0), 0)
            };
            offers.push(Offer {
                isp: p.company_name.clone(),
                product_id: p.id.0 as _,
                product_name: p.product_name.clone(),
                heading: p.product_name.clone(),
                list_price,
                discounted_price,
                discount_duration,
                campaign_descr: p
                    .price_comment
                    .as_ref()
                    .map_or(String::new(), |s| s.clone()),
                start_cost: p.start_cost,
                speed_up: p.speed_up as _,
                speed_down: p.speed_down as _,
                bind_time: p.binding_period.0 as _,
                leave_time: p.period_of_notice_months.0 as _,
            });
        }
        offers
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    id: StrInt,

    is_campaign: bool,
    #[serde(rename = "priceMRCIncVATComment")]
    price_comment: Option<String>,

    #[serde(rename = "descriptionBodyHTML")]
    description: String,
    #[serde(rename = "priceMRCIncVAT")]
    price_per_month: Sek,
    #[serde(rename = "startingCostIncVAT")]
    start_cost: Sek,
    #[serde(rename = "descriptionHeadline")]
    product_name: String,
    #[serde(rename = "serviceProviderName")]
    company_name: String,

    binding_period: Months,
    period_of_notice_months: Months,

    speed_up: u32,
    speed_down: u32,

    categories: Vec<ProductType>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
enum ProductType {
    Internet,
    Telephone,
    Tv,
    Other,
}

#[derive(Copy, Clone, PartialOrd, PartialEq, Deserialize)]
struct Mbit(#[serde(deserialize_with = "deserialize_option_number_from_string")] Option<u32>);

impl Debug for Mbit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(speed) => write!(f, "{speed} Mbit"),
            None => write!(f, "-"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
struct StrInt(i32);

impl<'de> Deserialize<'de> for StrInt {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let x = String::deserialize(deserializer)?;
        Ok(Self(x.parse().map_err(serde::de::Error::custom)?))
    }
}

#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sek(#[serde(deserialize_with = "deserialize_number_from_string")] i32);
impl Sek {
    pub fn i32(self) -> i32 {
        self.0
    }
}

impl From<i32> for Sek {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl Into<i32> for Sek {
    fn into(self) -> i32 {
        self.i32()
    }
}

impl Add for Sek {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}
impl Mul<i32> for Sek {
    type Output = i32;

    fn mul(self, rhs: i32) -> Self::Output {
        self.i32() * rhs
    }
}

impl Debug for Sek {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.0} SEK", self.i32())
    }
}
impl Display for Sek {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} kr", self.i32())
    }
}

impl FromStr for Sek {
    type Err = ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Deserialize)]
pub struct Months(#[serde(deserialize_with = "deserialize_number_from_string")] i32);

impl Debug for Months {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} months", self.0)
    }
}

#[test]
fn test_parse() -> Result<()> {
    let offers: ProductOffers = serde_json::from_reader(std::fs::File::open("fmt.json")?)?;
    dbg!(offers);

    Ok(())
}
