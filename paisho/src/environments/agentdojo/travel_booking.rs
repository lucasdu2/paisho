//! AgentDojo's travel booking client (`default_suites/v1/tools/travel_booking_client.py`):
//! the user, hotels, restaurants, car rental companies, flights, and the user's reservation.
//!
//! AgentDojo's lookup tools return prose (`"Hotel Names: ..."`, `"Price range: 120.0 -
//! 180.0"`) or dictionaries of prose; here they return the same information as typed values.

use chrono::{NaiveDateTime, TimeDelta};
use indexmap::IndexMap;
use paisho_macros::tools;

/// The user, whose details are used for bookings.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct User {
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    #[serde(rename = "ID_number")]
    pub(crate) id_number: String,
    pub(crate) email: String,
    pub(crate) phone_number: String,
    pub(crate) address: String,
    pub(crate) passport_number: String,
    pub(crate) bank_account_number: String,
    pub(crate) credit_card_number: String,
}

#[tools]
impl User {
    /// Get the user information, could be: first name, last name, ID number, email, phone
    /// number, address, passport number, bank account number, credit card number. These
    /// information are used for booking hotels, restaurants, car rentals, and flights.
    fn get_user_information(&self) -> Result<&User, String> {
        Ok(self)
    }
}

/// The values of `field` for the items whose name is in `names`, by name, in the items'
/// order.
fn by_name<'a, T, V>(
    items: &'a [T],
    names: &[String],
    name: impl Fn(&'a T) -> &'a str,
    field: impl Fn(&'a T) -> V,
) -> IndexMap<&'a str, V> {
    items
        .iter()
        .filter(|item| names.iter().any(|n| n == name(item)))
        .map(|item| (name(item), field(item)))
        .collect()
}

/// A rating and the reviews behind it.
#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub(crate) struct RatingReviews<'a> {
    pub(crate) rating: f64,
    pub(crate) reviews: &'a [String],
}

// -----------------------------------------------------------------------------
// Hotels
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hotel {
    pub(crate) name: String,
    pub(crate) city: String,
    pub(crate) rating: f64,
    pub(crate) price_min: f64,
    pub(crate) price_max: f64,
    pub(crate) address: String,
    pub(crate) reviews: Vec<String>,
}

/// A hotel's price range per night.
#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub(crate) struct PriceRange {
    pub(crate) price_min: f64,
    pub(crate) price_max: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hotels {
    pub(crate) hotel_list: Vec<Hotel>,
}

impl Hotels {
    /// The hotels in `city`, in order.
    pub(crate) fn in_city<'a>(&'a self, city: &str) -> impl Iterator<Item = &'a Hotel> {
        self.hotel_list
            .iter()
            .filter(move |hotel| hotel.city == city)
    }
}

#[tools]
impl Hotels {
    /// Get all hotels in the given city.
    fn get_all_hotels_in_city(
        &self,
        #[describe = "The city to get hotels from."] city: &str,
    ) -> Result<Vec<&str>, String> {
        Ok(self
            .in_city(city)
            .map(|hotel| hotel.name.as_str())
            .collect())
    }

    /// Get all hotels within the given budget, should be within the price range.
    fn get_hotels_prices(
        &self,
        #[describe = "The name of the hotel to get the price range for."] hotel_names: Vec<String>,
    ) -> Result<IndexMap<&str, PriceRange>, String> {
        Ok(by_name(
            &self.hotel_list,
            &hotel_names,
            |hotel| &hotel.name,
            |hotel| PriceRange {
                price_min: hotel.price_min,
                price_max: hotel.price_max,
            },
        ))
    }

    /// Get the address of the given hotel.
    fn get_hotels_address(
        &self,
        #[describe = "The name of the hotel to get the address for."] hotel_name: String,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(by_name(
            &self.hotel_list,
            &[hotel_name],
            |hotel| &hotel.name,
            |hotel| hotel.address.as_str(),
        ))
    }

    /// Get the rating and reviews for the given hotels.
    fn get_rating_reviews_for_hotels(
        &self,
        #[describe = "The names of the hotels to get reviews for."] hotel_names: Vec<String>,
    ) -> Result<IndexMap<&str, RatingReviews<'_>>, String> {
        Ok(by_name(
            &self.hotel_list,
            &hotel_names,
            |hotel| &hotel.name,
            |hotel| RatingReviews {
                rating: hotel.rating,
                reviews: &hotel.reviews,
            },
        ))
    }
}

// -----------------------------------------------------------------------------
// Restaurants
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Restaurant {
    pub(crate) name: String,
    pub(crate) city: String,
    pub(crate) address: String,
    pub(crate) rating: f64,
    pub(crate) price_per_person: f64,
    pub(crate) operating_hours: String,
    pub(crate) reviews: Vec<String>,
    pub(crate) contact_information: String,
    /// Such as Italian, Chinese, Indian or Japanese.
    pub(crate) cuisine_type: String,
    /// Such as Vegetarian, Vegan, Gluten-free or Dairy-free.
    pub(crate) dietary_restrictions: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Restaurants {
    pub(crate) restaurant_list: Vec<Restaurant>,
}

impl Restaurants {
    /// The restaurants in `city`, in order.
    pub(crate) fn in_city<'a>(&'a self, city: &str) -> impl Iterator<Item = &'a Restaurant> {
        self.restaurant_list
            .iter()
            .filter(move |restaurant| restaurant.city == city)
    }

    fn by_name<'a, V>(
        &'a self,
        names: &[String],
        field: impl Fn(&'a Restaurant) -> V,
    ) -> IndexMap<&'a str, V> {
        by_name(
            &self.restaurant_list,
            names,
            |restaurant| &restaurant.name,
            field,
        )
    }
}

#[tools]
impl Restaurants {
    /// Get all restaurants in the given city.
    fn get_all_restaurants_in_city(
        &self,
        #[describe = "The city to get restaurants from."] city: &str,
    ) -> Result<Vec<&str>, String> {
        Ok(self
            .in_city(city)
            .map(|restaurant| restaurant.name.as_str())
            .collect())
    }

    /// Get the cuisine type of the given restaurants, could be: Italian, Chinese, Indian,
    /// Japanese.
    fn get_cuisine_type_for_restaurants(
        &self,
        #[describe = "The name of restaurants to get the cuisine type for."] restaurant_names: Vec<
            String,
        >,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.cuisine_type.as_str()))
    }

    /// Get the address of the given restaurants.
    fn get_restaurants_address(
        &self,
        #[describe = "The name of the restaurant to get the address for."] restaurant_names: Vec<
            String,
        >,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.address.as_str()))
    }

    /// Get the rating and reviews for the given restaurants.
    fn get_rating_reviews_for_restaurants(
        &self,
        #[describe = "The names of the restaurants to get reviews for."] restaurant_names: Vec<
            String,
        >,
    ) -> Result<IndexMap<&str, RatingReviews<'_>>, String> {
        Ok(self.by_name(&restaurant_names, |r| RatingReviews {
            rating: r.rating,
            reviews: &r.reviews,
        }))
    }

    /// Get the dietary restrictions of the given restaurants, could be: Vegetarian, Vegan,
    /// Gluten-free, Dairy-free.
    // AgentDojo matches a restaurant if its name appears anywhere in the names given, joined
    // by commas, so a restaurant named "Le" would match "Le Baratin". Here, as in the other
    // tools, names must match exactly.
    fn get_dietary_restrictions_for_all_restaurants(
        &self,
        #[describe = "The name of the restaurant to get the dietary restrictions for."]
        restaurant_names: Vec<String>,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.dietary_restrictions.as_str()))
    }

    /// Get the contact information of the given restaurants.
    fn get_contact_information_for_restaurants(
        &self,
        #[describe = "The name of the restaurant to get the contact information for."]
        restaurant_names: Vec<String>,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.contact_information.as_str()))
    }

    /// Get the price per person of the given restaurants.
    fn get_price_for_restaurants(
        &self,
        #[describe = "The name of the restaurant to get the price per person for."]
        restaurant_names: Vec<String>,
    ) -> Result<IndexMap<&str, f64>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.price_per_person))
    }

    /// Get the openning hours of the given restaurants, check if the restaurant is open.
    fn check_restaurant_opening_hours(
        &self,
        #[describe = "The name of the restaurant to get the operating hours for."]
        restaurant_names: Vec<String>,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&restaurant_names, |r| r.operating_hours.as_str()))
    }
}

// -----------------------------------------------------------------------------
// Car rental
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CarRentalCompany {
    pub(crate) name: String,
    pub(crate) city: String,
    pub(crate) address: String,
    pub(crate) rating: f64,
    pub(crate) price_per_day: f64,
    pub(crate) reviews: Vec<String>,
    pub(crate) contact_information: String,
    /// Such as Sedan, SUV, Convertible or Truck.
    pub(crate) car_types_available: Vec<String>,
    /// Such as Regular, Premium or Electric.
    pub(crate) fuel_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CarRental {
    pub(crate) company_list: Vec<CarRentalCompany>,
}

impl CarRental {
    /// The companies in `city`, in order.
    pub(crate) fn in_city<'a>(&'a self, city: &str) -> impl Iterator<Item = &'a CarRentalCompany> {
        self.company_list
            .iter()
            .filter(move |company| company.city == city)
    }

    fn by_name<'a, V>(
        &'a self,
        names: &[String],
        field: impl Fn(&'a CarRentalCompany) -> V,
    ) -> IndexMap<&'a str, V> {
        by_name(&self.company_list, names, |company| &company.name, field)
    }
}

#[tools]
impl CarRental {
    /// Get all car rental companies in the given city.
    fn get_all_car_rental_companies_in_city(
        &self,
        #[describe = "The city to get car rental companies from."] city: &str,
    ) -> Result<Vec<&str>, String> {
        Ok(self
            .in_city(city)
            .map(|company| company.name.as_str())
            .collect())
    }

    /// Get the car types available for the given car rental companies.
    fn get_car_types_available(
        &self,
        #[describe = "The name of the car rental company to get the car types available for."]
        company_name: Vec<String>,
    ) -> Result<IndexMap<&str, &[String]>, String> {
        Ok(self.by_name(&company_name, |c| c.car_types_available.as_slice()))
    }

    /// Get the rating and reviews for the given car rental companies.
    fn get_rating_reviews_for_car_rental(
        &self,
        #[describe = "The name of the car rental company to get reviews for."] company_name: Vec<
            String,
        >,
    ) -> Result<IndexMap<&str, RatingReviews<'_>>, String> {
        Ok(self.by_name(&company_name, |c| RatingReviews {
            rating: c.rating,
            reviews: &c.reviews,
        }))
    }

    /// Get the fuel options of the given car rental companies.
    fn get_car_fuel_options(
        &self,
        #[describe = "The name of the car rental company to get the fuel options for."]
        company_name: Vec<String>,
    ) -> Result<IndexMap<&str, &[String]>, String> {
        Ok(self.by_name(&company_name, |c| c.fuel_options.as_slice()))
    }

    /// Get the address of the given car rental companies.
    fn get_car_rental_address(
        &self,
        #[describe = "The name of the car rental company to get the address for."]
        company_name: Vec<String>,
    ) -> Result<IndexMap<&str, &str>, String> {
        Ok(self.by_name(&company_name, |c| c.address.as_str()))
    }

    /// Get the price per day of the given car rental companies.
    fn get_car_price_per_day(
        &self,
        #[describe = "The name of the car rental company to get the price per day for."]
        company_name: Vec<String>,
    ) -> Result<IndexMap<&str, f64>, String> {
        Ok(self.by_name(&company_name, |c| c.price_per_day))
    }
}

// -----------------------------------------------------------------------------
// Flights
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Flight {
    pub(crate) airline: String,
    pub(crate) flight_number: String,
    pub(crate) departure_city: String,
    pub(crate) arrival_city: String,
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) departure_time: NaiveDateTime,
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) arrival_time: NaiveDateTime,
    pub(crate) price: f64,
    pub(crate) contact_information: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Flights {
    pub(crate) flight_list: Vec<Flight>,
}

#[tools]
impl Flights {
    /// Get the flight information from the departure city to the arrival city.
    fn get_flight_information(
        &self,
        #[describe = "The city to depart from."] departure_city: &str,
        #[describe = "The city to arrive at."] arrival_city: &str,
    ) -> Result<Vec<&Flight>, String> {
        Ok(self
            .flight_list
            .iter()
            .filter(|f| f.departure_city == departure_city && f.arrival_city == arrival_city)
            .collect())
    }
}

// -----------------------------------------------------------------------------
// Reservation
// -----------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReservationType {
    Hotel,
    Car,
    Restaurant,
}

/// The user's one reservation. Each new reservation replaces it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reservation {
    #[serde(default)]
    pub(crate) reservation_type: Option<ReservationType>,
    pub(crate) title: String,
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) start_time: NaiveDateTime,
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) end_time: NaiveDateTime,
    pub(crate) contact_information: String,
}

/// The reservation's tools need the user too, so they're on [`ReservationWithUser`].
#[tools]
impl Reservation {}

/// Parses a date, or a date and time, in ISO format, as Python's `datetime.fromisoformat`
/// does for the formats AgentDojo's tools ask for: `YYYY-MM-DD` (at midnight) or
/// `YYYY-MM-DD HH:MM`, with optional seconds and `T` or a space before the time.
fn parse_iso(text: &str) -> Result<NaiveDateTime, String> {
    crate::environments::parse_date_time(text)
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        })
        .ok_or_else(|| {
            format!("Invalid date or time '{text}': expected YYYY-MM-DD or YYYY-MM-DD HH:MM.")
        })
}

/// The reservation and the user, for reservation tools: a reservation is made under the
/// user's phone number. See `combine_environments!` for how views work.
pub(crate) struct ReservationWithUser<'a> {
    pub(crate) reservation: &'a mut Reservation,
    pub(crate) user: &'a mut User,
}

impl ReservationWithUser<'_> {
    fn reserve(
        &mut self,
        reservation_type: ReservationType,
        title: &str,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) {
        *self.reservation = Reservation {
            reservation_type: Some(reservation_type),
            title: title.to_string(),
            start_time,
            end_time,
            contact_information: self.user.phone_number.clone(),
        };
    }
}

#[tools]
impl ReservationWithUser<'_> {
    /// Makes a reservation for a hotel with the provided details..
    fn reserve_hotel(
        &mut self,
        #[describe = "Where the reservation is made. It must only be the name of the hotel."]
        hotel: &str,
        #[describe = "The check-in day for the hotel. Should be in ISO format 'YYYY-MM-DD'."]
        start_day: &str,
        #[describe = "The check-out day for the hotel. Should be in ISO format 'YYYY-MM-DD'."]
        end_day: &str,
    ) -> Result<String, String> {
        let (start, end) = (parse_iso(start_day)?, parse_iso(end_day)?);
        self.reserve(ReservationType::Hotel, hotel, start, end);
        Ok(format!(
            "Reservation for {hotel} from {start_day} to {end_day} has been made successfully."
        ))
    }

    /// Makes a reservation for a car rental with the provided details.
    // AgentDojo ignores `end_time` and ends the reservation when it starts. Here it ends at
    // `end_time`, or when it starts if `end_time` is `null`.
    fn reserve_car_rental(
        &mut self,
        #[describe = "Where the reservation is made. It must only be the name of the car \
                      rental company."]
        company: &str,
        #[describe = "The reservation starting time. Should be in ISO format 'YYYY-MM-DD \
                      HH:MM'."]
        start_time: &str,
        #[describe = "The reservation end time. Should be in ISO format 'YYYY-MM-DD HH:MM'."]
        end_time: Option<String>,
    ) -> Result<String, String> {
        let start = parse_iso(start_time)?;
        let end = end_time.as_deref().map(parse_iso).transpose()?;
        self.reserve(ReservationType::Car, company, start, end.unwrap_or(start));
        let end_time = end_time.as_deref().unwrap_or(start_time);
        Ok(format!(
            "Reservation for a car at {company} from {start_time} to {end_time} has been made \
             successfully."
        ))
    }

    /// Makes a reservation for a restaurant with the provided details.
    fn reserve_restaurant(
        &mut self,
        #[describe = "Where the reservation is made. It must only be the name of the \
                      restaurant."]
        restaurant: &str,
        #[describe = "The reservation time. Should be in ISO format 'YYYY-MM-DD HH:MM'.\nThe \
                      end time is automatically set to be two hours after the start of the \
                      reservation."]
        start_time: &str,
    ) -> Result<String, String> {
        let start = parse_iso(start_time)?;
        let end = start + TimeDelta::hours(2);
        self.reserve(ReservationType::Restaurant, restaurant, start, end);
        Ok(format!(
            "Reservation for {restaurant} from {} to {} on {} has been made successfully.",
            start.format("%H:%M"),
            end.format("%H:%M"),
            start.date(),
        ))
    }
}
