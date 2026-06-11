"""Every number and fixed string M0 depends on, in one place (NUMBERS RECAP).

datetime.now() is forbidden in generation paths: all timestamps derive from
FIXED_EPOCH. All randomness flows from one seeded random.Random(SEED).
"""

SEED_DEFAULT = 42

# Entity counts.
N_PEOPLE = 120
N_GROUPS = 14
N_AGENTS = 4
N_SOURCES = 5
N_DOCUMENTS = 600
N_DEPARTMENTS = 8
N_SITES = 2

# Document mix minimums.
N_SOP_FAMILIES = 25            # each with >= 2 versions, both kept in corpus
N_SOP_VERSIONS = 2
MIN_EFFECTIVE_VERSION_TRAPS = 12
N_QUALITY_RECORDS = 40
N_HR_RECORDS = 30
N_BOARD_MINUTES = 12
N_CUSTOMER_DOCS = 60

# Trap minimums (effective-version / mosaic / confused-deputy / manager / site).
MIN_MOSAIC_PAIRS = 10
MIN_CONFUSED_DEPUTY = 15
MIN_MANAGER_OVERREACH = 8
MIN_CROSS_SITE = 6

# Oracle gates: a mostly-open corpus can't catch leaks.
ALLOW_RATE_CEILING = 0.35
RESTRICTED_SPECIAL_ALLOW_CEILING = 0.05

# Document bodies.
BODY_WORDS_MIN = 150
BODY_WORDS_MAX = 400

# Fixed epoch (no wall clocks anywhere in generation).
FIXED_EPOCH_ISO = "2026-01-05T09:00:00Z"
FIXED_EPOCH_DATE = (2026, 1, 5)

COMPANY_NAME = "Bryremead Distribution Ltd"

DEPARTMENTS = [
    "Quality & Compliance",
    "Warehouse Operations",
    "Pharmacy Services",
    "Finance",
    "IT",
    "HR",
    "Sales & Accounts",
    "Executive",
]

SITES = ["site_keldonbury", "site_withermoor"]

SOURCES = ["docstore", "wiki", "mail_lite", "hr_system", "quality_system"]

SENSITIVITIES = ["public", "internal", "confidential", "restricted", "special_category"]

# Group ids: one per department plus six cross-cutting groups = 14.
DEPT_GROUP_IDS = {
    "Quality & Compliance": "grp_quality_compliance",
    "Warehouse Operations": "grp_warehouse_operations",
    "Pharmacy Services": "grp_pharmacy_services",
    "Finance": "grp_finance",
    "IT": "grp_it",
    "HR": "grp_hr",
    "Sales & Accounts": "grp_sales_accounts",
    "Executive": "grp_executive",
}
CROSS_GROUP_IDS = [
    "grp_qa_release",
    "grp_board",
    "grp_payroll_admins",
    "grp_incident_responders",
    "grp_gdp_responsible_persons",
    "grp_contractors",
]

# The deny-by-default probe (P-4): a person who is organizationally present
# but belongs to NO groups and holds no granting attributes.
VOID_PRINCIPAL_ID = "p_void"

# Denylist: real companies/people that must never appear in any output file.
# Seeds required by spec plus the largest real pharmaceutical distributors /
# wholesalers recalled. Scanned case-insensitively as substrings.
DENYLIST = [
    "Mawdsley",
    "Mawdsleys",
    "Bachy",
    "CGI",
    "McKesson",
    "Cencora",
    "AmerisourceBergen",
    "Cardinal Health",
    "Walgreens",
    "Boots",
    "Alliance Healthcare",
    "AAH Pharmaceuticals",
    "Phoenix Group",
    "PHOENIX Pharma",
    "Celesio",
    "Sinopharm",
    "Shanghai Pharmaceuticals",
    "China Resources Pharmaceutical",
    "Jointown",
    "Zuellig",
    "Medipal",
    "Alfresa",
    "Suzuken",
    "Toho Pharmaceutical",
    "Galenica",
    "Tamro",
    "Oriola",
    "Henry Schein",
    "Owens & Minor",
    "Sigma Pharmaceuticals",
    "Lloyds Pharmacy",
]
