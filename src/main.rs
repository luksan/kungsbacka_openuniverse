// #![allow(dead_code)]

mod parse;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate tabular;

use anyhow::Result;

use crate::parse::{parse_details_page, parse_internet_overview_page};
use clap::{App, Arg, ArgMatches, SubCommand};
use regex::Regex;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const INTERNET_URL: &str = "http://kungsbacka.openuniverse.se/kungsbackastadsnat/internet/privat/";
const INTERNET_FILE: &str = "internet_rs.html";
const PAKET_URL: &str = "http://kungsbacka.openuniverse.se/kungsbackastadsnat/paket/privat/";
const PAKET_FILE: &str = "paket_rs.html";
const PRODUCT_PAGE_URL: &str =
    "http://kungsbacka.openuniverse.se/kungsbackastadsnat/details.xhtml?productId=<prod_id>";
const PRODUCT_PAGE_FILE: &str = "product_<prod_id>.html";

type Months = u8;
type SEK = u16;
type MBit = u16;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Campaign {
    Yes(String),
    No,
    CheckDetails,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    isp: String,
    product_id: u32,
    product_name: String,
    heading: String,
    campaign: Campaign,
    list_price: SEK,
    start_cost: SEK,
    speed_up: MBit,
    speed_down: MBit,
    bind_time: Months,
    leave_time: Months,
}

impl Offer {
    pub fn calc_cost_2nd_year(&self) -> SEK {
        self.list_price * 12
    }
    pub fn calc_cost_1st_year(&self) -> SEK {
        lazy_static! {
            static ref MONTHS: Regex = Regex::new(r"\b(\d\d?|tre) ?mån").unwrap();
            static ref PRICE: Regex = Regex::new(r"(\d+) ?kr").unwrap();
        }
        match &self.campaign {
            Campaign::No => self.calc_cost_2nd_year() + self.start_cost,
            Campaign::CheckDetails => panic!("The details should be resolved during update"),
            Campaign::Yes(campaign) => {
                let months;
                if let Some(months_match) = MONTHS.captures(campaign) {
                    if &months_match[1] == "tre" {
                        months = 3
                    } else {
                        months = months_match[1].parse().expect(campaign);
                    }
                } else if campaign.contains("ett halvår") {
                    months = 6;
                } else {
                    eprintln!("Campaign parse failed: {}", campaign);
                    return 0;
                }
                let price: SEK;
                if let Some(price_match) = PRICE.captures(campaign) {
                    price = price_match[1].parse().expect(campaign);
                } else if campaign.contains("halva priset")
                    || campaign.contains("Halva priset")
                    || campaign.contains("½ priset")
                {
                    price = self.list_price / 2;
                } else {
                    eprintln!("Failed to parse price reduction: {}", campaign);
                    return 0;
                }
                months * price + (12 - months) * self.list_price + self.start_cost
            }
        }
    }
    pub fn speed_str(&self) -> String {
        format!("{}/{}", self.speed_down, self.speed_up)
    }
}

/// Sorts a Vec<Offer> according to the given field names
macro_rules! sort_offers {
    ($offers:expr, $head:ident) => {
        $offers.sort_by(|a, b| a.$head.cmp(&b.$head))
    };

    ($offers:expr, $head:ident, $($fields:ident),+) => {
        $offers.sort_by(|a, b| a.$head.cmp(&b.$head)$(.then(a.$fields.cmp(&b.$fields)))+)
    };
}

fn download() -> Result<()> {
    println!("Downloading offer listings");
    {
        let response = attohttpc::get(INTERNET_URL).send()?;
        let internet_file = File::create(INTERNET_FILE)?;
        response.write_to(internet_file)?;
    }
    {
        let response = attohttpc::get(PAKET_URL).send()?;
        let package_file = File::create(PAKET_FILE)?;
        response.write_to(package_file)?;
    }
    Ok(())
}

fn fetch_details_page(product_id: u32) -> Result<PathBuf> {
    let pid_str = product_id.to_string();
    let url = PRODUCT_PAGE_URL.replace("<prod_id>", &pid_str);
    let filename: PathBuf = PRODUCT_PAGE_FILE.replace("<prod_id>", &pid_str).into();
    let response = attohttpc::get(url).send()?;
    let file = File::create(&filename)?;
    response.write_to(file)?;
    Ok(filename)
}

fn update(args: &ArgMatches) -> Result<()> {
    if !args.is_present("no-download") {
        download()?;
    }
    let mut internet_offers = parse_internet_overview_page(load_file(INTERNET_FILE)?.as_ref());

    let mut remove = Vec::new();
    for i in 0..internet_offers.len() {
        let offer = &internet_offers[i];
        if offer.campaign == Campaign::CheckDetails {
            let filename = fetch_details_page(offer.product_id)?;
            let mut sub_offers = parse_details_page(load_file(filename)?.as_ref())?;
            internet_offers.append(&mut sub_offers);
            remove.push(i);
        }
    }
    for r in remove.iter().rev() {
        internet_offers.swap_remove(*r);
    }
    internet_offers.sort_by_key(|offer| offer.product_id);

    let x = serde_json::to_vec_pretty(&internet_offers)?;
    println!("Saving JSON data");
    let mut json_file = File::create("offers.json")?;
    json_file.write_all(&x)?;
    println!("Update OK");
    Ok(())
}

fn dump(_args: &ArgMatches) -> Result<()> {
    let mut offers = load_offers_from_json()?;

    sort_offers!(offers, isp, speed_down, speed_up);

    let mut table = tabular::Table::new("{:<} {:>} {:>} kr {:>} kr {:>} kr");
    table.add_row(row!("ISP", "DL/UL", "År 1", "År 2", "1+2"));
    for offer in &offers {
        let y1 = offer.calc_cost_1st_year();
        let y2 = offer.calc_cost_2nd_year();
        table.add_row(row! {
            &offer.isp.replace(" ", "_"),
            offer.speed_str(),
            y1,
            y2,
            y1+y2,
        });
    }
    println!("{}", table);

    println!("{} offers in database", offers.len());
    Ok(())
}

fn load_file<P: AsRef<Path>>(filename: P) -> Result<String> {
    let mut ret = String::with_capacity(10_000);
    File::open(filename)?.read_to_string(&mut ret)?;
    Ok(ret)
}

fn load_offers_from_json() -> Result<Vec<Offer>> {
    let ret: Vec<Offer> = serde_json::from_reader(File::open("offers.json")?)?;
    Ok(ret)
}

fn main() -> Result<()> {
    let matches = App::new("Kungsbacka Openuniverse priskoll")
        .version("0.1")
        .author("Lukas Sandström")
        .about("Laddar ner erbjudanden från Openuniverse och jämför priser")
        .subcommand(
            SubCommand::with_name("update")
                .about("Ladda ner priser från Open Universe")
                .arg(
                    Arg::with_name("no-download")
                        .long("no-download")
                        .help("Don't go online to download prices, use the cache only."),
                ),
        )
        .subcommand(SubCommand::with_name("dump").about("Print the current offers in the database"))
        .get_matches();

    match matches.subcommand() {
        ("update", Some(args)) => update(args)?,
        ("dump", Some(args)) => dump(args)?,
        _ => (), // No subcommand given
    };

    Ok(())
}
