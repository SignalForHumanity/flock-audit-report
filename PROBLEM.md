---
slug: flock-audit-report
title: Web page that summarizes Flock ALPR search-audit CSVs from public-records requests, privately in the browser, even when they have millions of rows
verdict: build
---

## Problem

Across the US, residents, local "DeFlock" groups and small newsrooms file
public-records requests for the search audits of Flock Safety license-plate
cameras. These files (Organization Audit, Network Audit, and the
transparency portal's "Public Search Audit") are the main evidence that local
cameras are searched by out-of-state agencies, federal agencies, and for
immigration enforcement, protests, or "traffic infraction" reasons that
state law forbids. That evidence has led to cancelled contracts and fired
officers.

The trouble is the size and shape of the files:

- A Network Audit is often 1–3 million rows per month (200 MB each). Signal
  Akron received 33.4 million rows in 19 CSV files
  (https://signalakron.org/akron-flock-cameras-used-for-non-local-immigration-enforcement/).
  Excel and Google Sheets stop at 1,048,576 rows. DeFlockILM's own guide
  says New Hanover's file "will make a spreadsheet cry" and tells readers to
  use pandas (https://deflockilm.org/read-flock-audit-logs/). RANGE Media
  warns "you will likely need a more robust software than Microsoft Excel"
  (https://www.rangemedia.co/public-flock-data/).
- One search fans out across many networks, so the same search ID appears in
  several agencies' files. Counting rows overcounts. DeFlockILM removed
  6.6 million duplicates from 10.5 million rows
  (https://deflockilm.org/nc-flock-aggregate-methodology/).
- The questions are always the same (who searched, from which state, federal
  or not, with what reason, how many vague or blank reasons, how many
  immigration or First Amendment keywords, how many nationwide searches), but
  each group writes its own one-off script: DeFlockILM's
  `nc_flock_aggregate.py`, hsandorf/deflockdekalb, jkat510/Flock,
  Matthew-Griffith/kzoo-flock-audit, DeFlock Woodland's audit.

People without Python end up with truncated spreadsheets or no analysis.

## Who benefits

Local activists and DeFlock chapters, journalists at small outlets, and
council members' staff who receive Flock audit CSVs. They find the page via a
link in the guides they already read (DeFlockILM, Have I Been Flocked's audit
log page, r/FlockSurveillance, DeflockYourCity's toolkit). They open
the page on any computer, pick the CSV files, and get a report in seconds.
Nothing is uploaded, so they can use it on records they have not published.

## Existing solutions

- **Have I Been Flocked** (https://haveibeenflocked.com/): a large public
  aggregation with plate search and national immigration and First Amendment
  reports. You submit files to it and wait. It does not analyze your own
  file privately, and its views cover only the data it has published.
- **Cantica Systems' Michigan ledger** and **Eyes On Flock**: archives of
  transparency portals for some regions only.
- **DeFlockILM's `nc_flock_aggregate.py`**, **hsandorf/deflockdekalb**,
  **alprwatch.org's foia.py**: Python scripts for one group's files. They
  need Python and a terminal, and each handles only part of the analysis.
- **Excel / Google Sheets pivot tables**: what the guides recommend for small
  files. They fail above 1,048,576 rows and cannot dedupe across files.
- **Generic big-CSV tools** (Power Query, CSV viewers, DuckDB, pandas): able
  to load the data, but you need to know what to compute, and the skills.
- **Atlanta News First's "AI audit tool"** in Georgia is used by agencies,
  not the public.
- GitHub and crates.io: nothing for analyzing Flock audit logs. The repos
  are camera detectors (ESP32 and BLE scanners) and records archives.

## Why build anything

Nothing lets a non-programmer take a multi-million-row Flock audit and get
the standard accountability numbers privately. The existing scripts prove the
analysis is small and well understood. What is missing is a form that
volunteers can run, that dedupes across files and that never sends records
anywhere.

## Smallest useful intervention

A static web page (Rust compiled to WebAssembly) that streams one or more
Flock audit CSVs from disk in chunks and prints one report:

- per-file row counts, with a warning when a file stops at Excel's row limit;
- unique searches after removing duplicate search IDs across files;
- date range and searches per month;
- searching agencies ranked, with state and federal classification, and the
  share from outside a home state the user picks;
- reason categories, blank and vague reasons, and missing case numbers;
- keyword flags for immigration, First Amendment activity and reproductive
  care, with the matching reasons and agencies;
- scope (how many networks each search covered) and search types;
- when names and plates are not redacted, only counts (busiest operators
  as "operator 1, 2…" and plates searched 10+ times), never names or plates.

It also offers a plain-text copy of the report to paste into a memo or
article.

## Success criterion

- On a real UNC Pembroke Network Audit excerpt, the totals and top reasons
  match an independent Python count.
- Feeding the same file twice gives the same report as feeding it once.
- A 1-million-row file is processed in the browser in well under a minute
  on an ordinary laptop, with the chunked reader keeping memory flat
  except for the dedup set.
- No plate number or operator name ever appears in the output (tested).

## Maintenance

A static page on GitHub Pages costs nothing to run. Flock could rename or add
columns. Column lookup is by header name with aliases, so a rename means
adding one alias. New keyword lists or vague-reason words are one-line
changes. There are no network calls or keys.

## Decision

Build. The need comes from the public guides that tell volunteers to
use pandas, and from the many one-off scripts. No existing tool gives
non-programmers private, deduplicated analysis of files too large for a
spreadsheet. The page reports aggregates only and hides operator names and
plates, so it holds police use of surveillance to account without
becoming a tool for tracking people.

## Also considered

- stalkerware-network-check (detect stalkerware on an at-risk person's phone): SpyGuard (maintained TinyCheck fork), SpyCheck, Amnesty's MVT and Echap's stalkerware-indicators already exist and are recommended by the Coalition Against Stalkerware.
- data-broker-optout-helper (help doxxing-threatened journalists remove their home address from people-search sites): a crowded field, with Permission Slip, California's DROP, DataPurge (free and open source), the Big Ass Data Broker Opt-Out List and many paid services.
- perspective-api-replacement (free toxicity scoring for small community moderators after Google's Perspective API ends in 2026): it needs an ML model and a server to be useful, and open-source and commercial drop-in replacements (OpenModeration and others) already exist.
