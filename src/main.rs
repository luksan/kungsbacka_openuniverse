#![allow(dead_code)]

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate tabular;

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::json_portal::{ProductOffers, Sek};

mod json_portal;

const INTERNET_URL: &str = "https://selfservice.ip-only.se/api/consumer-selfservice-backend/v1/public/service-offers?accessId=1137975&isCompany=false&onlyOrderableOffers=false&priorityOption=ALL_OFFERS";
const INTERNET_FILE: &str = "internet.json";
const PRODUCT_PAGE_URL: &str =
    "https://portal.openuniverse.se/best%C3%A4ll/tj%C3%A4nster/1137975/produkt-detaljer/<prod_id>";
const PRODUCT_PAGE_FILE: &str = "product_<prod_id>.html";

type Months = i32;
type SEK = Sek;
type MBit = u16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    isp: String,
    product_id: u32,
    product_name: String,
    heading: String,
    campaign_descr: String,
    list_price: SEK,
    discounted_price: SEK,
    discount_duration: Months,
    start_cost: SEK,
    speed_up: MBit,
    speed_down: MBit,
    bind_time: Months,
    leave_time: Months,
}

impl Offer {
    pub fn calc_cost_2nd_year(&self) -> Sek {
        (self.list_price * 12).into()
    }
    pub fn calc_cost_1st_year(&self) -> SEK {
        (self.discounted_price * self.discount_duration as i32
            + (12 - self.discount_duration) * self.list_price.i32()
            + self.start_cost.i32())
        .into()
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

    let response = attohttpc::get(INTERNET_URL).send().context(INTERNET_URL)?;
    let internet_file = File::create(INTERNET_FILE).context(INTERNET_FILE)?;
    response.write_to(internet_file)?;

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

fn update(no_download: bool) -> Result<()> {
    if !no_download {
        download().context("Failed to download offers")?;
    }
    // let mut internet_offers = parse_internet_overview_page(load_file(INTERNET_FILE)?.as_ref());
    let products = ProductOffers::from_file(INTERNET_FILE)?;

    let mut internet_offers = products.get_internet_offers();
    internet_offers.sort_by_key(|offer| offer.product_id);

    let x = serde_json::to_vec_pretty(&internet_offers)?;
    println!("Saving JSON data");
    let mut json_file = File::create("offers.json")?;
    json_file.write_all(&x)?;
    println!("Update OK");
    Ok(())
}

fn dump() -> Result<()> {
    let mut offers = load_offers_from_json()?;

    sort_offers!(offers, isp, speed_down, speed_up);

    let mut table = tabular::Table::new("{:<} {:>} {:>} {:>} {:>}");
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
    let filename = "offers.json";
    let ret: Vec<Offer> =
        serde_json::from_reader(File::open(&filename).context("Failed to open file.")?)
            .context("Failed to parse file.")?;
    Ok(ret)
}

#[derive(Parser)]
#[clap(author, version, about)]
struct CmdlineOpts {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the current offers in the database
    Dump,
    /// Ladda ner priser från Open Universe
    Update {
        /// Don't go online to download prices, use the cache only.
        #[clap(short = 'n', long = "no-download")]
        no_download: bool,
    },
}

fn main() -> Result<()> {
    let args: CmdlineOpts = Parser::parse();

    match args.command {
        Commands::Dump => dump(),
        Commands::Update { no_download } => update(no_download),
    }
}
