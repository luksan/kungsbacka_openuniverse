use anyhow::Result;

use crate::parse::ParseErrors::{
    BddNotFound, CampaignError, HeadingError, IspError, ListPriceError, OtherError,
    PurchaseInfoError,
};
use crate::{Campaign, MBit, Months, Offer, SEK};
// use markup5ever;
use regex::Regex;
use soup::{NodeExt, QueryBuilderExt, Soup};
use thiserror::Error;

const BDD_ID: &str = "data-bdd-id";

type NodeHandle = std::rc::Rc<markup5ever::rcdom::Node>;

#[derive(Error, Debug)]
pub enum ParseErrors {
    #[error("Some parsing error")]
    OtherError,

    #[error("Failed to find bdd id with value {0}")]
    BddNotFound(String),

    #[error("ISP parse error {0}")]
    IspError(String),

    #[error("list price error {0}")]
    ListPriceError(String),

    #[error("Purchase info error {0}")]
    PurchaseInfoError(String),

    #[error("Heading error {0}")]
    HeadingError(String),

    #[error("Campaign error")]
    CampaignError,
}

pub fn parse_internet_overview_page(html: &str) -> Vec<Offer> {
    let doc_root = Soup::new(html);
    let offers = doc_root.attr(BDD_ID, "serviceOfferItem").find_all();
    offers.filter_map(|offer| parse_offer(offer).ok()).collect()
}

pub fn parse_details_page(html: &str) -> Result<Vec<Offer>, ParseErrors> {
    let doc_root = Soup::new(html);
    let main_node = doc_root
        .tag("article")
        .class("serviceOfferDetail")
        .find()
        .ok_or_else(|| BddNotFound("serviceOfferDetail".into()))?;

    let isp = get_isp(&main_node)?;
    let product_id = get_product_id(&find_bdd(&main_node, "productIcon-image")?)?;
    let product_name: String = find_bdd(&main_node, "serviceHeader-text")?
        .text()
        .trim()
        .into();

    let heading: String = find_bdd(&main_node, "serviceMainHeader-text")?
        .text()
        .trim()
        .into();

    let list_price = find_bdd(&main_node, "serviceBaseOffer-text")?
        .text()
        .split(' ')
        .next()
        .ok_or_else(|| ListPriceError("No list price".into()))?
        .parse()
        .map_err(|_err| ListPriceError("List price parse error".into()))?;

    let (speed_down, speed_up) = get_speed(&product_name, &heading).ok_or(OtherError)?;

    let mut ret = Vec::new();
    for offer in main_node.attr(BDD_ID, "offerDetailsArea").find_all() {
        let (start_cost, bind_time, leave_time) = get_purchase_info(
            &offer
                .class("campaignOfferDetails")
                .find()
                .ok_or_else(|| PurchaseInfoError("campaignOfferDetails not found".into()))?,
        )?;

        let campaign = Campaign::Yes(
            find_bdd(&offer, "campaignDescriptionText")?
                .text()
                .trim()
                .into(),
        );

        ret.push(Offer {
            isp: isp.clone(),
            product_id,
            product_name: product_name.clone(),
            heading: heading.clone(),
            campaign,
            list_price,
            start_cost,
            speed_up,
            speed_down,
            bind_time,
            leave_time,
        });
    }
    Ok(ret)
}

fn parse_offer(offer: NodeHandle) -> Result<Offer> {
    let heading = get_heading(&offer)?;
    let product_name = get_product_name(&offer)?;
    let (speed_down, speed_up) = get_speed(&product_name, &heading).ok_or(OtherError)?;
    let (start_cost, bind_time, leave_time) =
        get_purchase_info(&find_bdd(&offer, "serviceOfferPurchaseInformation")?)?;

    Ok(Offer {
        isp: get_isp(&offer)?,
        product_id: get_product_id(&find_bdd(&offer, "service-offer-details")?)?,
        product_name,
        heading,
        campaign: get_campaign(&offer)?,
        list_price: get_list_price(&offer)?,
        start_cost,
        speed_up,
        speed_down,
        bind_time,
        leave_time,
    })
}

fn get_purchase_info(node: &NodeHandle) -> Result<(SEK, Months, Months), ParseErrors> {
    let text = node.text();
    lazy_static! {
        static ref START_RE: Regex = Regex::new(r"Startavgift.*?(\d+)").unwrap();
        static ref BIND_RE: Regex = Regex::new(r"Bindningstid.*?(\d+)").unwrap();
        static ref LEAVE_RE: Regex = Regex::new(r"Uppsägningstid.*?(\d+)").unwrap();
    }
    let ok = || {
        Some((
            START_RE.captures(&text)?[1].parse().ok()?,
            BIND_RE.captures(&text)?[1].parse().ok()?,
            LEAVE_RE.captures(&text)?[1].parse().ok()?,
        ))
    };
    ok().ok_or(PurchaseInfoError(text))
}

fn get_campaign(offer: &NodeHandle) -> Result<Campaign, ParseErrors> {
    // Check that the <article> has "campaign" as class, otherwise the campaignDetails are irrelevant
    if offer.recursive(false).class("campaign").find().is_none() {
        return Ok(Campaign::No);
    }

    let details = find_bdd(offer, "campaignDetailsText")?;
    Ok(
        if details
            .recursive(false)
            .class("multipleCampaigns")
            .find()
            .is_some()
        {
            Campaign::CheckDetails
        } else {
            Campaign::Yes(
                details
                    .text()
                    .trim()
                    .splitn(2, '\n')
                    .nth(1)
                    .ok_or(CampaignError)?
                    .trim()
                    .to_owned(),
            )
        },
    )
}

/// Parse the speed from the product name or heading. Returns (Downlink, Uplink)
fn get_speed(product_name: &str, heading: &str) -> Option<(MBit, MBit)> {
    lazy_static! {
        static ref SPEED_RE: Regex = Regex::new(r"(\d+)[_/](\d+)").unwrap();
        static ref SIMPLE_SPEED_RE: Regex = Regex::new(r"(\d+)[_/](\d+)").unwrap();
    }
    let s2 = |text: &str| -> Option<(MBit, MBit)> {
        let capt = SPEED_RE.captures(&text)?;
        Some((capt[1].parse().ok()?, capt[2].parse().ok()?))
    };

    s2(&product_name).or_else(|| s2(heading)).or_else(|| {
        SIMPLE_SPEED_RE.find(&heading).and_then(|speed_match| {
            speed_match
                .as_str()
                .parse()
                .ok()
                .map(|speed: MBit| (speed, speed))
        })
    });
    if let Some(speeds) = s2(&product_name) {
        return Some(speeds);
    };

    if let Some(speeds) = s2(&heading) {
        return Some(speeds);
    };

    if let Some(speed) = SIMPLE_SPEED_RE.find(&heading) {
        let speed = speed.as_str().parse().ok()?;
        return Some((speed, speed));
    }

    None
}

fn get_list_price(offer: &NodeHandle) -> Result<SEK, ParseErrors> {
    let node = find_bdd(offer, "endUserPrice")?;

    node.text()
        .split_ascii_whitespace()
        .next()
        .ok_or_else(|| ListPriceError(node.text()))?
        .parse()
        .map_err(|_err| ListPriceError(node.text()))
}

fn get_heading(offer: &NodeHandle) -> Result<String, ParseErrors> {
    let node = find_bdd(offer, "serviceOfferHeading")?;
    node.tag("h4")
        .find()
        .map(|h4| h4.text().trim().to_owned())
        .ok_or_else(|| HeadingError(node.display()))
}

fn get_product_name(offer: &NodeHandle) -> Result<String, ParseErrors> {
    Ok(find_bdd(offer, "productName")?.text())
}

fn product_id_from_str_end(urlish: &str) -> Result<u32, ParseErrors> {
    urlish
        .rsplit("productId=")
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or(OtherError)
}

fn get_product_id(node: &NodeHandle) -> Result<u32, ParseErrors> {
    let attrs = node.attrs();

    product_id_from_str_end(
        attrs
            .get("href")
            .or_else(|| attrs.get("src"))
            .ok_or(OtherError)?,
    )
}

fn get_isp(offer: &NodeHandle) -> Result<String, ParseErrors> {
    let node = find_bdd(offer, "serviceOfferImage") // main listing page
        .or_else(|_err| find_bdd(offer, "productProviderLogo-image"))?; // details page
    node.attrs()
        .get("src")
        .ok_or_else(|| IspError(node.display()))?
        .rsplit("isp=")
        .next()
        .map(|sref| sref.to_owned())
        .ok_or_else(|| IspError(node.display()))
}

fn find_bdd(node: &NodeHandle, attr_value: &str) -> Result<NodeHandle, ParseErrors> {
    node.attr(BDD_ID, attr_value)
        .find()
        .ok_or_else(|| BddNotFound(attr_value.to_owned()))
}

#[cfg(test)]
mod tests {
    use crate::parse::*;
    use crate::Campaign;

    #[test]
    fn test_parse_html() {
        let offers = parse_internet_overview_page(include_str!("../internet_rs.html").as_ref());
        println!("Found {} offers.", offers.len());
        for offer in offers {
            match &offer.campaign {
                Campaign::No => (),
                Campaign::CheckDetails => {
                    println!("{:?}", offer);
                }
                Campaign::Yes(_) => {
                    println!("{:?}", offer.campaign);
                }
            }
        }
    }

    #[test]
    fn test_parse_details() {
        let offers = parse_details_page(include_str!("../product_525.html").as_ref());
        if offers.is_err() {
            println!("{:?}", offers);
            return;
        }
        for offer in offers.unwrap() {
            println!("{:?}", offer);
        }
    }
}
