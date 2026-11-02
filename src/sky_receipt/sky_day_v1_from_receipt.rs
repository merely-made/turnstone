// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyDayV1 {
    pub(super) fn from_receipt(receipt: &SkyDerivationReceipt) -> Self {
        let day = receipt.day();
        Self {
            date: day.date().to_string(),
            time_zone: day.time_zone_name().to_owned(),
            start_utc: day.start_utc().to_string(),
            end_utc: day.end_utc().to_string(),
            start_tt: TtJulianDateV1::from_tt(day.start_tt()),
            end_tt: TtJulianDateV1::from_tt(day.end_tt()),
        }
    }

    pub(super) fn validate(&self) -> Result<(TtJulianDateV1, TtJulianDateV1), SkyReceiptJsonError> {
        let date: Date = self
            .date
            .parse()
            .map_err(|_| invalid(format!("invalid Sky civil date '{}'", self.date)))?;
        if self.date != date.to_string() {
            return Err(invalid("Sky civil date must use Jiff's canonical form"));
        }
        // The stored UTC boundaries are the durable result of resolving this
        // label when the receipt was minted. A later tzdb may retire the label
        // or assign different civil rules, so reading never resolves it again.
        validate_time_zone_identifier(&self.time_zone)?;
        let start_utc: Timestamp = self
            .start_utc
            .parse()
            .map_err(|_| invalid("invalid Sky civil-day UTC start"))?;
        let end_utc: Timestamp = self
            .end_utc
            .parse()
            .map_err(|_| invalid("invalid Sky civil-day UTC end"))?;
        if self.start_utc != start_utc.to_string() {
            return Err(invalid("Sky civil-day UTC start must use canonical form"));
        }
        if self.end_utc != end_utc.to_string() {
            return Err(invalid("Sky civil-day UTC end must use canonical form"));
        }
        if start_utc >= end_utc {
            return Err(invalid("Sky civil-day UTC bounds must increase"));
        }

        self.start_tt.validate("civil-day TT start")?;
        self.end_tt.validate("civil-day TT end")?;
        let expected_start_tt = TtJulianDateV1::from_tt(tt_from_utc(start_utc));
        let expected_end_tt = TtJulianDateV1::from_tt(tt_from_utc(end_utc));
        if self.start_tt != expected_start_tt {
            return Err(invalid(
                "Sky civil-day two-part TT start does not match its UTC boundary",
            ));
        }
        if self.end_tt != expected_end_tt {
            return Err(invalid(
                "Sky civil-day two-part TT end does not match its UTC boundary",
            ));
        }
        Ok((expected_start_tt, expected_end_tt))
    }
}
