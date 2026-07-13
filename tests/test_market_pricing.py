import unittest

from pipeline.skills.market_pricing_facts import pricing_to_facts


class MarketPricingFactTests(unittest.TestCase):
    def test_magicbricks_pricing_keeps_source_and_basis(self):
        facts = pricing_to_facts({
            "found": True,
            "primary_source_name": "MagicBricks",
            "primary_source_url": "https://www.magicbricks.com/prestige-raintree-park",
            "configurations": [
                {
                    "bhk": "3BHK",
                    "sqft_range": "2004-2482",
                    "price_range_lakh": "259-353",
                    "price_per_sqft": "11600-13200",
                    "source_name": "MagicBricks",
                    "source_url": "https://www.magicbricks.com/prestige-raintree-park",
                }
            ],
            "avg_price_per_sqft": 13500,
            "market_status": "under_construction",
        })

        by_key = {fact.key: fact for fact in facts}
        self.assertIn("pricing_source", by_key)
        self.assertIn("pricing_3bhk", by_key)
        self.assertEqual(
            by_key["pricing_3bhk"].source.url,
            "https://www.magicbricks.com/prestige-raintree-park",
        )
        self.assertEqual(
            by_key["pricing_3bhk"].value["data"]["basis"],
            "marketplace_asking_price",
        )
        self.assertEqual(by_key["pricing_3bhk"].confidence, 0.75)

    def test_crore_price_range_is_parsed_as_lakhs(self):
        facts = pricing_to_facts({
            "found": True,
            "primary_source_name": "MagicBricks",
            "primary_source_url": "https://www.magicbricks.com/prestige-raintree-park",
            "configurations": [
                {
                    "bhk": "3BHK",
                    "sqft_range": "2004-2482",
                    "price_range_lakh": "2.59-3.53 Cr",
                    "price_per_sqft": "11600-13200",
                    "source_name": "MagicBricks",
                    "source_url": "https://www.magicbricks.com/prestige-raintree-park",
                }
            ],
        })

        keys = {fact.key for fact in facts}
        self.assertIn("pricing_3bhk", keys)

    def test_implausible_price_math_is_not_written(self):
        facts = pricing_to_facts({
            "found": True,
            "primary_source_name": "MagicBricks",
            "primary_source_url": "https://www.magicbricks.com/example",
            "configurations": [
                {
                    "bhk": "3BHK",
                    "sqft_range": "2000-2200",
                    "price_range_lakh": "100-120",
                    "price_per_sqft": "20000-22000",
                    "source_name": "MagicBricks",
                    "source_url": "https://www.magicbricks.com/example",
                }
            ],
            "avg_price_per_sqft": 21000,
            "market_status": "under_construction",
        })

        keys = {fact.key for fact in facts}
        self.assertNotIn("pricing_3bhk", keys)
        self.assertNotIn("price_per_sqft", keys)
        self.assertNotIn("market_status", keys)

    def test_source_less_pricing_is_not_written(self):
        facts = pricing_to_facts({
            "found": True,
            "configurations": [
                {
                    "bhk": "3BHK",
                    "sqft_range": "2004-2482",
                    "price_range_lakh": "259-353",
                    "price_per_sqft": "11600-13200",
                }
            ],
            "avg_price_per_sqft": 13500,
        })

        self.assertEqual(facts, [])


if __name__ == "__main__":
    unittest.main()
