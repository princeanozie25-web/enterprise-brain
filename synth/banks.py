"""All fictional content for M0: name banks, vocabulary, and the
document-body templating system.

Every string in this module is invented. Nothing here (and nothing this
module can render) may contain any entry from constants.DENYLIST, which is
scanned case-insensitively as substrings over all output.

Determinism: the only randomness is the random.Random instance the caller
passes in. This module never seeds, never reads wall clocks, never touches
uuid/os.urandom, and never iterates unsorted sets.

Sensitivity respect (DO-NOT): each doc_type's sentence bank mentions only its
own domain. Salary language lives only in the hr_record bank, board language
only in board_minutes, batch/disposition language only in quality_record.
Bodies draw ONLY from these generic banks plus the caller-provided slots.
"""

from __future__ import annotations

import random

from synth import constants

# ---------------------------------------------------------------------------
# Name banks (all invented; UK flavour).
# ---------------------------------------------------------------------------

FIRST_NAMES = [
    "Aldwyn", "Alric", "Anselm", "Averil", "Bedwyn",
    "Berrith", "Branoc", "Brenna", "Caradoc", "Carwen",
    "Ceridwen", "Clemmie", "Dagny", "Delmara", "Drystan",
    "Eadlin", "Edrith", "Eira", "Elowen", "Emrys",
    "Enid", "Evander", "Faelan", "Fenella", "Ferrin",
    "Ffion", "Galwyn", "Gethin", "Gwendra", "Haelwen",
    "Hartley", "Hesper", "Idony", "Ines", "Isolde",
    "Jessamy", "Jocasta", "Kelda", "Kerensa", "Kestrel",
    "Lorcan", "Lowri", "Lysander", "Maren", "Meriel",
    "Merritt", "Morwen", "Nerys", "Ninian", "Odeline",
    "Orin", "Osric", "Padrig", "Perrin", "Petra",
    "Quenby", "Quillon", "Rhianwen", "Rosalind", "Rowena",
    "Sabran", "Selwyn", "Seren", "Sorrel", "Tamsin",
    "Tegan", "Thandry", "Tobin", "Una", "Verity",
    "Vesper", "Wendel", "Wilfreda", "Wynne", "Yestin",
    "Yseult", "Zinnia", "Eldric", "Briallen", "Corwin",
]

LAST_NAMES = [
    "Aldmere", "Ashfeld", "Astergill", "Barrowden", "Bexcombe",
    "Birchstead", "Bramhurst", "Brigmoor", "Burfen", "Byrnshaw",
    "Caldermere", "Carnleigh", "Caswold", "Coldhurst", "Cranfeld",
    "Crowmere", "Danthorpe", "Draymoor", "Dunscombe", "Dwerrith",
    "Eldenshaw", "Elmswick", "Embergill", "Eskfeld", "Farrowdale",
    "Fenwright", "Fernshaw", "Frethwick", "Garslade", "Gorsefen",
    "Glenharrow", "Gartholme", "Haldercroft", "Harkfeld", "Hazelgarth",
    "Holdenfen", "Hurstwold", "Hentlow", "Ingleshaw", "Ivenmoor",
    "Keldwick", "Kilnshaw", "Kervenholt", "Lamfeld", "Larkmoor",
    "Lindenmere", "Lockgarth", "Marclough", "Merrowdale", "Mossbeck",
    "Marnthwaite", "Nethergrave", "Norcombe", "Nornbeck", "Oakharrow",
    "Orleshaw", "Osmerley", "Otterfeld", "Pellbrook", "Pendlowe",
    "Pargrove", "Purslade", "Quarfeld", "Quillmere", "Radmoor",
    "Redgarth", "Rushbeck", "Rendlewick", "Skellbourne", "Stanmere",
    "Swinfeld", "Sorrelwick", "Tarnwold", "Thistlegarth", "Tredmoor",
    "Turlowe", "Twymarsh", "Urswald", "Wexbourne", "Whinfeld",
    "Wrenfold", "Wynngarth", "Yarrowfeld", "Yewbeck",
]

# ---------------------------------------------------------------------------
# Role bank: first element of each list is the department-head role.
# Keys are exactly constants.DEPARTMENTS, in the same order.
# ---------------------------------------------------------------------------

ROLE_BANK = {
    "Quality & Compliance": [
        "Head of Quality & Compliance",
        "GDP Responsible Person",
        "GDP Compliance Officer",
        "Quality Assurance Specialist",
        "Deviation Coordinator",
        "Document Control Officer",
        "Supplier Qualification Auditor",
    ],
    "Warehouse Operations": [
        "Head of Warehouse Operations",
        "Cold Chain Supervisor",
        "Goods-In Team Leader",
        "Picking & Packing Operative",
        "Inventory Control Analyst",
        "Despatch Coordinator",
        "Forklift Operative",
    ],
    "Pharmacy Services": [
        "Head of Pharmacy Services",
        "Responsible Pharmacist",
        "Pharmacy Technician",
        "Dispensary Assistant",
        "Clinical Governance Lead",
        "Medicines Information Officer",
    ],
    "Finance": [
        "Head of Finance",
        "Financial Controller",
        "Management Accountant",
        "Payroll Officer",
        "Accounts Payable Clerk",
        "Accounts Receivable Clerk",
        "Credit Control Analyst",
    ],
    "IT": [
        "Head of IT",
        "Infrastructure Engineer",
        "Service Desk Analyst",
        "Applications Developer",
        "Information Security Lead",
        "Data Platform Engineer",
    ],
    "HR": [
        "Head of HR",
        "HR Business Partner",
        "Recruitment Coordinator",
        "Learning & Development Officer",
        "Employee Relations Adviser",
        "HR Systems Administrator",
    ],
    "Sales & Accounts": [
        "Head of Sales & Accounts",
        "Key Account Manager",
        "Territory Sales Executive",
        "Customer Service Representative",
        "Sales Operations Analyst",
        "Contracts Administrator",
        "Pricing Analyst",
    ],
    "Executive": [
        "Chief Executive Officer",
        "Chief Operating Officer",
        "Chief Financial Officer",
        "Company Secretary",
        "Strategy Director",
        "Executive Assistant",
    ],
}

# ---------------------------------------------------------------------------
# Fictional pharmaceutical product names (all invented; no real drug names).
# ---------------------------------------------------------------------------

PRODUCT_TERMS = [
    "Veltrazine", "Corbamol", "Quindrazol", "Bremvotane", "Kelvarol",
    "Dovrasel", "Pelluvane", "Nerravex", "Torbelan", "Morvaplex",
    "Saffrenol", "Trevodal", "Wexlorin", "Ostrivane", "Calverix",
    "Dunmerol", "Halverine", "Quellorase", "Embrathol", "Vintarex",
    "Goldrane", "Selbavine", "Tarnovine", "Ruskelon", "Pondrale",
    "Mistraval", "Ferrowyn", "Locravine", "Edrovane", "Burrelain",
]

# ---------------------------------------------------------------------------
# Fictional pharmacy / clinic customer names.
# ---------------------------------------------------------------------------

CUSTOMER_NAMES = [
    "Hollowbeck Pharmacy", "Asterfen Chemists", "Birchmere Health Centre",
    "Bramblegate Pharmacy", "Caldewick Clinic", "Carrowdale Dispensary",
    "Coppermill Surgery", "Dunmarsh Clinic", "Elderwick Pharmacy",
    "Fallowmere Surgery", "Fernbridge Health Centre", "Gablethorpe Chemists",
    "Gildersfen Pharmacy", "Gorsewood Clinic", "Harrowgill Chemists",
    "Hazeldene Care Home", "Hollyfen Dispensary", "Ivybeck Surgery",
    "Kestrelgate Pharmacy", "Kilnmere Health Centre", "Lanthorne Chemists",
    "Larchfeld Clinic", "Lindengate Pharmacy", "Maplewick Surgery",
    "Merrowgate Dispensary", "Mossfen Pharmacy", "Netherfold Health Centre",
    "Norngate Chemists", "Oakfen Clinic", "Otterwell Pharmacy",
    "Pellmere Surgery", "Pippinfold Chemists", "Quarrydene Health Centre",
    "Rookwell Dispensary", "Rushgate Pharmacy", "Saltmere Clinic",
    "Sorrelfen Chemists", "Tarnwell Pharmacy", "Thistledown Care Home",
    "Wexmere Surgery", "Willowfen Health Centre", "Wrengate Pharmacy",
    "Yarrowdene Clinic",
]

SITE_DISPLAY = {
    "site_keldonbury": "Keldonbury",
    "site_withermoor": "Withermoor",
}

DOC_TYPES = [
    "sop",
    "quality_record",
    "hr_record",
    "board_minutes",
    "customer_account",
    "wiki_page",
    "mail_thread",
    "general",
]

# ---------------------------------------------------------------------------
# Title topic banks (used when the caller supplies no topic slot).
# ---------------------------------------------------------------------------

_SOP_TOPICS = [
    "Cold Chain Receipt and Putaway",
    "Ambient Storage Housekeeping",
    "Returns Assessment and Segregation",
    "Despatch and Vehicle Loading",
    "Temperature Excursion Response",
    "Stock Rotation and Expiry Checking",
    "Picking Accuracy Verification",
    "Goods-In Documentation Checks",
    "Segregation Area Management",
    "Waste and Damaged Stock Handling",
    "Pest Control Monitoring",
]

_QUALITY_TOPICS = [
    "Periodic Batch Documentation Review",
    "Supplier Qualification Assessment",
    "Temperature Mapping Summary",
    "Returned Goods Assessment",
    "Annual Self-Inspection Finding",
    "Transport Lane Verification",
]

_WIKI_TOPICS = [
    "Warehouse Shift Handover Basics",
    "Ordering Stationery and Consumables",
    "Using the Delivery Scheduling Screen",
    "New Starter IT Setup",
    "Site Parking and Access",
    "Raising a Facilities Ticket",
]

_MAIL_TOPICS = [
    "Delivery Schedule Change",
    "Stock Availability Query",
    "Meeting Room Booking",
    "Rota Swap Request",
    "System Maintenance Window",
    "Training Session Reminder",
]

_GENERAL_TOPICS = [
    "Site Notice",
    "Process Update",
    "Service Bulletin",
    "Team Announcement",
    "Operational Reminder",
    "Policy Refresh",
]

_RECORD_KIND_TITLES = {
    "salary_review": "Salary Review",
    "grievance": "Grievance",
    "absence_summary": "Absence Summary",
}

_RECORD_KIND_OPENERS = {
    "salary_review": "This record documents the outcome of a scheduled salary review meeting.",
    "grievance": "This record documents a formal grievance raised and handled under the company procedure.",
    "absence_summary": "This record summarises recorded absence for the reporting period covered.",
}

# ---------------------------------------------------------------------------
# Sentence banks: one per doc_type, each restricted to its own domain.
# Placeholders: {product} {customer} {site} (filled deterministically via rng).
# ---------------------------------------------------------------------------

_SOP_SENTENCES = [
    "This standard operating procedure defines the controls applied at the {site} distribution centre to maintain product integrity during routine operations.",
    "All personnel assigned to this activity must complete documented training against the current version before performing the task unsupervised.",
    "Temperature-monitored areas are checked at the start of each shift and the readings are recorded on the designated log sheet.",
    "Equipment used in this process is calibrated to the maintenance schedule and tagged with its next calibration due date.",
    "Any departure from the steps described here must be reported to the duty supervisor before the affected stock is moved.",
    "Records generated under this procedure are retained for the period defined in the records retention schedule.",
    "The process owner reviews this document on a two-yearly cycle, or sooner where a change to operations requires it.",
    "Stock movements are completed in the warehouse management system before any physical relocation takes place.",
    "Protective equipment appropriate to the task is worn at all times within the designated handling areas.",
    "Where {product} or any other temperature-sensitive line is handled, the validated packing configuration must be used.",
    "Access to the segregation area is limited to trained staff listed on the current authorisation register.",
    "Vehicles are inspected on arrival for cleanliness, security seals, and evidence of temperature control throughout transit.",
    "Discrepancies identified during receipt are documented immediately and the consignment is held pending investigation.",
    "The flow of goods through goods-in, storage, picking, and despatch is mapped in the appendix to this procedure.",
    "Cleaning of the storage areas is carried out to the documented schedule and verified by the shift lead.",
    "This procedure applies to all permanent and temporary staff working within the operations covered by its scope.",
]

_QUALITY_SENTENCES = [
    "The batch record was reviewed for completeness against the master template prior to the disposition decision.",
    "Incoming inspection confirmed that the consignment documentation matched the purchase order and the supplier despatch note.",
    "Temperature data from the storage location was reviewed for the full holding period and no excursions were identified.",
    "Sampling was performed in accordance with the documented plan and the retained samples were logged.",
    "The investigation considered handling, storage, and transport stages to establish whether product quality was affected.",
    "Corrective and preventive actions arising from this record are tracked to closure in the quality system.",
    "The supplier certificate of conformity was verified and filed with the batch documentation.",
    "Shelf life remaining at the point of receipt satisfied the minimum agreed for onward distribution.",
    "Labelling and outer packaging were examined and found consistent with the approved specification.",
    "The quality team confirmed that segregation requirements were maintained throughout the assessment period.",
    "A trend check against previous receipts of the same product line showed no recurring pattern requiring escalation.",
    "All entries in this record were made contemporaneously and countersigned by a second trained reviewer.",
    "The decision rationale is documented in the assessment section and approved by the releasing officer.",
    "Storage conditions at the {site} facility remained within the validated range for the duration under review.",
]

_HR_SENTENCES = [
    "This record was created and maintained in line with the company people-records procedure.",
    "The meeting was attended by the employee, the line manager, and a note-taker from the HR team.",
    "Both parties confirmed the accuracy of the notes before the record was finalised.",
    "Supporting documents referenced in this record are held in the secure personnel file.",
    "The outcome was communicated in writing within the timescale set out in the relevant policy.",
    "Any appeal against the outcome must be lodged in accordance with the published appeals process.",
    "Access to this record is limited to authorised members of the HR function and the individuals concerned.",
    "The next scheduled review point is recorded in the HR system against the employee profile.",
    "Salary information in this record reflects the position at the date the record was finalised.",
    "The discussion covered objectives, development needs, and any adjustments agreed for the period ahead.",
    "Where actions were assigned, owners and target dates are listed at the end of this record.",
    "This entry supersedes any informal notes made during earlier conversations on the same matter.",
]

_BOARD_SENTENCES = [
    "The minutes of the previous meeting were approved as an accurate record without amendment.",
    "Apologies for absence were received and noted at the opening of the meeting.",
    "The board reviewed management information covering trading, service levels, and operational risk.",
    "Declarations of interest were invited and none were raised in relation to the items under discussion.",
    "The directors considered the report and agreed the recommendations set out in the paper.",
    "An update on the regulatory inspection programme was received and the preparedness actions were endorsed.",
    "The risk register was reviewed and the movement in principal risks since the last meeting was noted.",
    "The board requested a further paper on the matter for consideration at the next scheduled meeting.",
    "Resolutions recorded in these minutes were passed unanimously unless otherwise stated.",
    "The chair summarised the agreed actions and confirmed owners for each before closing the meeting.",
    "Matters arising from previous meetings were reviewed and the completed actions were closed.",
    "The meeting was declared quorate and the agenda was adopted as circulated.",
]

_CUSTOMER_SENTENCES = [
    "The account operates on the standard wholesale terms agreed at onboarding, as recorded in the account file.",
    "Orders are placed through the online portal and confirmed by the customer service team on the same working day.",
    "Deliveries are scheduled on the agreed route days, with temperature-controlled transport used where the order requires it.",
    "The account contact details were verified at the most recent service review.",
    "Credit arrangements for this account are managed by the accounts team under the published credit policy.",
    "Regular lines for this account include {product}, which is supplied subject to availability.",
    "Service issues raised by the customer are logged, investigated, and answered under the complaints procedure.",
    "Proof-of-delivery records for this account are retained and available on request.",
    "The customer licence and registration details were checked and recorded before first supply.",
    "Returns from this account are handled under the returned-goods procedure and assessed before any restocking decision.",
    "The relationship is reviewed periodically by the assigned account manager and any changes to requirements are recorded.",
    "Out-of-hours requests are handled through the emergency supply arrangement described in the service schedule.",
]

_WIKI_SENTENCES = [
    "This page is maintained by the owning team and was last reviewed as part of the routine documentation sweep.",
    "Use the linked request form to raise changes; edits are applied after review by the page owner.",
    "The summary below reflects current working practice at both distribution sites.",
    "New starters should read this page alongside their induction checklist.",
    "Common questions about this topic are collected at the foot of the page with short answers.",
    "Where this page conflicts with a controlled procedure, the controlled procedure takes precedence.",
    "Screenshots are illustrative and may differ slightly from the current system version.",
    "Related pages are listed in the sidebar for readers who need wider context.",
    "Suggestions for improving this page can be posted in the documentation channel.",
    "The terminology section explains abbreviations used across the operations teams.",
    "Step-by-step instructions are kept deliberately short; follow the links for detail on each stage.",
    "Archived versions of this page are retained automatically and can be restored by the administrators.",
]

_MAIL_SENTENCES = [
    "Thanks for the quick turnaround on this; the replies below cover the outstanding points.",
    "Following up on the earlier note, the team has confirmed the revised arrangement for next week.",
    "Please review the summary and reply with any corrections by the end of the working day.",
    "Adding the relevant colleagues to this thread so everyone has the same picture.",
    "The short version: the change is agreed, and the detail is in the paragraph below.",
    "As discussed on the call, the actions are listed with owners and dates.",
    "Flagging for visibility rather than action; no response is needed unless something looks wrong.",
    "Could you confirm receipt so the request can be progressed to the next stage?",
    "Trimming the older quoted text to keep this thread readable.",
    "The standing order for {customer} discussed earlier has been rescheduled to the next route day.",
    "The attachment mentioned earlier has been replaced with the corrected version.",
    "If anything is unclear, a brief call tomorrow morning would be the fastest way to resolve it.",
    "Closing the loop on this thread; the original query has been resolved and no further action remains.",
]

_GENERAL_SENTENCES = [
    "This notice applies to all staff across both distribution sites and takes effect immediately.",
    "Further detail will be shared through the usual team briefings over the coming days.",
    "Questions about the content of this document should be directed to the issuing team.",
    "The arrangements described here will be reviewed after an initial settling-in period.",
    "Staff are reminded to check the noticeboards and the intranet for subsequent updates.",
    "No action is required beyond reading and acknowledging this communication.",
    "The change supports the company-wide programme of continuous improvement.",
    "Line managers will cascade the practical details relevant to each team.",
    "Feedback gathered during the previous cycle informed the approach described here.",
    "A summary version of this document is available for display in shared areas.",
    "The effective date and version of this communication are shown in the footer.",
    "Thanks are recorded to the colleagues who contributed to preparing this material.",
]

_SENTENCE_BANKS = {
    "sop": _SOP_SENTENCES,
    "quality_record": _QUALITY_SENTENCES,
    "hr_record": _HR_SENTENCES,
    "board_minutes": _BOARD_SENTENCES,
    "customer_account": _CUSTOMER_SENTENCES,
    "wiki_page": _WIKI_SENTENCES,
    "mail_thread": _MAIL_SENTENCES,
    "general": _GENERAL_SENTENCES,
}

# ---------------------------------------------------------------------------
# Templating helpers
# ---------------------------------------------------------------------------


def _check_doc_type(doc_type: str) -> None:
    if doc_type not in _SENTENCE_BANKS:
        raise ValueError(f"unknown doc_type: {doc_type!r}")


def _fill(rng: random.Random, template: str) -> str:
    """Resolve generic-vocabulary placeholders deterministically via rng."""
    out = template
    if "{product}" in out:
        out = out.replace("{product}", rng.choice(PRODUCT_TERMS))
    if "{customer}" in out:
        out = out.replace("{customer}", rng.choice(CUSTOMER_NAMES))
    if "{site}" in out:
        out = out.replace("{site}", SITE_DISPLAY[rng.choice(constants.SITES)])
    return out


def _mandatory_parts(doc_type: str, slots: dict) -> list[str]:
    """Slot-bearing sentences the body must visibly include (verbatim slots).

    Built purely from the caller-provided slots: no rng, no invented detail.
    """
    parts: list[str] = []
    if doc_type == "sop":
        if slots.get("procedure_code"):
            parts.append(
                f"This procedure is identified as {slots['procedure_code']} "
                "and forms part of the controlled document set."
            )
        if slots.get("topic"):
            parts.append(f"Scope of this procedure: {slots['topic']}.")
        if slots.get("parameter_text"):
            # The effective-version trap diffs on this exact string: it must
            # appear verbatim exactly once, so no other sentence may carry it.
            parts.append(
                "The controlled operating parameter for this activity is "
                f"{slots['parameter_text']}."
            )
    elif doc_type == "quality_record":
        if slots.get("batch_code"):
            parts.append(f"Batch reference: {slots['batch_code']}.")
        if slots.get("product"):
            parts.append(f"Product concerned: {slots['product']}.")
        if slots.get("disposition"):
            parts.append(
                "Following review, the batch disposition is recorded as "
                f"{slots['disposition']}."
            )
    elif doc_type == "hr_record":
        if slots.get("subject_name"):
            parts.append(f"Subject of record: {slots['subject_name']}.")
        if slots.get("record_kind"):
            parts.append(f"Record category: {slots['record_kind']}.")
            opener = _RECORD_KIND_OPENERS.get(slots["record_kind"])
            if opener:
                parts.append(opener)
        if slots.get("salary_band_text"):
            parts.append(f"Current salary band: {slots['salary_band_text']}.")
    elif doc_type == "board_minutes":
        if slots.get("quarter_label"):
            parts.append(
                f"Minutes of the board meeting for {slots['quarter_label']} "
                "are set out below."
            )
        agenda = slots.get("agenda_items")
        if agenda:
            parts.append("The agenda comprised: " + "; ".join(agenda) + ".")
    elif doc_type == "customer_account":
        if slots.get("customer_name"):
            parts.append(f"Account name: {slots['customer_name']}.")
        if slots.get("account_code"):
            parts.append(f"Account reference: {slots['account_code']}.")
    elif doc_type == "wiki_page":
        if slots.get("topic"):
            parts.append(f"This page covers {slots['topic']}.")
    elif doc_type == "mail_thread":
        if slots.get("topic"):
            parts.append(f"Thread subject: {slots['topic']}.")
    elif doc_type == "general":
        if slots.get("topic"):
            parts.append(f"This communication concerns {slots['topic']}.")
    # Mosaic support, any doc_type: each stat line appears verbatim.
    for line in slots.get("stat_lines", []):
        parts.append(str(line))
    return parts


def render_title(rng: random.Random, doc_type: str, slots: dict) -> str:
    """Deterministic document title for the given doc_type and slots."""
    _check_doc_type(doc_type)
    topic = slots.get("topic")
    if doc_type == "sop":
        subject = topic or rng.choice(_SOP_TOPICS)
        code = slots.get("procedure_code")
        if code:
            return f"SOP {code}: {subject}"
        return f"Standard Operating Procedure: {subject}"
    if doc_type == "quality_record":
        subject = slots.get("product") or rng.choice(_QUALITY_TOPICS)
        batch = slots.get("batch_code")
        if batch:
            return f"Quality Record {batch}: {subject}"
        return f"Quality Record: {subject}"
    if doc_type == "hr_record":
        kind = _RECORD_KIND_TITLES.get(slots.get("record_kind", ""), "Personnel Record")
        name = slots.get("subject_name")
        if name:
            return f"HR Record ({kind}): {name}"
        return f"HR Record ({kind})"
    if doc_type == "board_minutes":
        label = slots.get("quarter_label")
        if label:
            return f"Board Minutes: {label}"
        return "Board Minutes"
    if doc_type == "customer_account":
        name = slots.get("customer_name") or "Customer Account Profile"
        code = slots.get("account_code")
        if code:
            return f"Customer Account: {name} ({code})"
        return f"Customer Account: {name}"
    if doc_type == "wiki_page":
        return f"Wiki: {topic or rng.choice(_WIKI_TOPICS)}"
    if doc_type == "mail_thread":
        return f"RE: {topic or rng.choice(_MAIL_TOPICS)}"
    # general
    return f"Notice: {topic or rng.choice(_GENERAL_TOPICS)}"


def render_body(rng: random.Random, doc_type: str, slots: dict) -> str:
    """Deterministic document body.

    Guarantees (for the reasonable slot sizes M0 generates):
      - word count between constants.BODY_WORDS_MIN and BODY_WORDS_MAX;
      - every provided doc-type slot value appears verbatim;
      - sop parameter_text appears exactly once;
      - sentences come only from this doc_type's own bank plus the slots.
    """
    _check_doc_type(doc_type)
    target = rng.randint(constants.BODY_WORDS_MIN, constants.BODY_WORDS_MAX)

    parts = _mandatory_parts(doc_type, slots)
    word_count = sum(len(p.split()) for p in parts)

    bank = _SENTENCE_BANKS[doc_type]
    order: list[int] = []
    while word_count < target:
        if not order:
            order = list(range(len(bank)))
            rng.shuffle(order)
        sentence = _fill(rng, bank[order.pop()])
        n = len(sentence.split())
        if word_count + n > constants.BODY_WORDS_MAX:
            break
        parts.append(sentence)
        word_count += n
    return " ".join(parts)
