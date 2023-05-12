use std::fmt::{Debug, Formatter};
use std::num::ParseIntError;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};

use crate::{CampaignInfo, MBit, Offer};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductOffers {
    is_done: bool,
    next_page: u32,
    products: Vec<Product>,
}

impl ProductOffers {
    pub fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        serde_json::from_reader(std::fs::File::open(file.as_ref())?).context("Failed to parse json")
    }

    pub fn get_internet_offers(&self) -> Vec<Offer> {
        let mut offers = vec![];
        for p in self
            .products
            .iter()
            .filter(|p| p.prod_type == ProductType::Internet)
        {
            assert_eq!(p.services.len(), 1);
            let base_offer = Offer {
                isp: p.isp_name.clone(),
                product_id: p.pascal_case.product_id,
                product_name: p.pascal_case.product_name_web.clone(),
                heading: p.pascal_case.product_header.clone(),
                campaign: CampaignInfo::No,
                list_price: p.end_user_price.0 .0 as _,
                start_cost: p.end_user_start_price.0 .0 as _,
                speed_up: p.services[0].up_speed.0.unwrap() as MBit,
                speed_down: p.services[0].down_speed.0.unwrap() as MBit,
                bind_time: p.binding_time.0 .0 as _,
                leave_time: p.pascal_case.period_of_notice.0 .0 as _,
            };
            for c in &p.campaigns {
                let mut offer = base_offer.clone();

                offer.campaign = CampaignInfo::Yes(c.campaign_text.clone());
                offer.start_cost = c.end_user_start_price.0 .0 as _;
                offer.product_id = c.product_id;
                offer.bind_time = c.binding_time.0 .0 as _;
                offer.leave_time = c.period_of_notice.0 .0 as _;

                offers.push(offer)
            }
            offers.push(base_offer);
        }
        offers
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    binding_time: Months,
    campaigns: Vec<Campaign>,
    delivery_information: String,
    end_user_price: Sek,
    end_user_start_price: Sek,
    introduction: String,
    is_company: u32, // actually a bool
    is_package: bool,
    isp_name: String,
    other_information: String,
    #[serde(flatten)]
    pascal_case: ProductPascalCase,
    services: Vec<Service>,
    site: StrInt,
    target_group: u32,
    terms_text: String,
    #[serde(rename = "type")]
    prod_type: ProductType,
    master_product_id: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
enum ProductType {
    AdditionalServicesInternet,
    Internet,
    Telefoni,
    Tv,
    Other,
    Package,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProductPascalCase {
    package_type: u32,
    period_of_notice: Months,
    preferred_product: StrInt, // bool??
    product_description: String,
    product_header: String,
    product_id: u32,
    product_name_web: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Service {
    down_speed: Mbit,
    service_group: u32,
    service_id: u32,
    #[serde(rename(deserialize = "speed"))]
    speed: String,
    up_speed: Mbit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Campaign {
    binding_time: Months,
    campaign_code: String,
    campaign_header: String,
    campaign_text: String,
    end_user_price: Sek,
    end_user_start_price: Sek,
    period_of_notice: Months,
    product_id: u32,
    valid_in_number_of_months: Months,
}

#[derive(Copy, Clone, PartialOrd, PartialEq)]
struct Mbit(Option<u32>);

impl<'de> Deserialize<'de> for Mbit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let x = String::deserialize(deserializer)?;
        Ok(Self(if x.is_empty() {
            None
        } else {
            Some(x.parse().map_err(serde::de::Error::custom)?)
        }))
    }
}

impl Debug for Mbit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(speed) => write!(f, "{speed} Mbit"),
            None => write!(f, "-"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
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

#[derive(Copy, Clone, PartialEq, Deserialize)]
pub struct Sek(StrInt);

impl Debug for Sek {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.0} SEK", self.0 .0)
    }
}

impl FromStr for Sek {
    type Err = ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(StrInt(s.parse()?)))
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Deserialize)]
pub struct Months(StrInt);

impl Debug for Months {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} months", self.0 .0)
    }
}

#[test]
fn test_parse() -> Result<()> {
    let offers: ProductOffers = serde_json::from_reader(std::fs::File::open("fmt.json")?)?;
    dbg!(offers);

    Ok(())
}
