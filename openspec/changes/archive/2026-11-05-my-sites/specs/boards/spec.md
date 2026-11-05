## ADDED Requirements

### Requirement: Each person has their own list of job sites, On or Off
The app SHALL keep, for each person, whether each approved job site is On or Off, and who set it
(the app or the person). Hiring systems and company pages SHALL NOT be on the list. A person's own
choice SHALL never be overwritten by the app, and turning a site back keeps it their choice. A site
that needs a key before it can be read SHALL be listed and SHALL NOT be turned On until the key is
set. A site that is not on the list, or that does not exist, SHALL be answered as not found.

#### Scenario: The person turns a site Off
- **WHEN** a person turns Remote OK Off
- **THEN** it is Off and stays Off when their goals change, and the list says nothing else about who set it

#### Scenario: A site that needs a key
- **WHEN** a person turns On a site that needs a key it does not have
- **THEN** the answer is a conflict `needs_key` and the site stays Off

#### Scenario: A company page
- **WHEN** a person asks to turn On a company page or a hiring system
- **THEN** the answer is not found, and it is not on their list

### Requirement: The app turns on the sites that fit the person's goals
For each approved site that needs no key and that the person has not chosen, the app SHALL set it On
or Off from the person's goals and the site's tags, and SHALL set it again whenever the list is
read, so changed goals and a site approved later are picked up. A site SHALL be On when: for a
remote-first site open worldwide or an all-jobs site, the person would take a remote job; for a
European site, they live in Europe or would move within the EU or anywhere, and the site has remote
jobs unless they would take office days; for a site in their country, the site has remote jobs
unless they would take office days; for a site built around a stack, a word of it is among their
skills or roles; for a narrow trade, their roles name that kind of work. A site whose kinds of jobs
do not match the kind of work their roles name SHALL be Off. A person with no answered goals SHALL
have every site Off, written nowhere. The app SHALL read the goals only through the port searches
use.

#### Scenario: A remote engineer in Lisbon
- **WHEN** a person in Lisbon who works remote only, as a backend engineer, has no choice on any site
- **THEN** the remote worldwide, the European sites with remote jobs, their country's with remote jobs and the all-jobs sites are On, and a Ruby site, a design site and a site of office jobs are Off

#### Scenario: The goals change
- **WHEN** the same person now wants office work
- **THEN** the sites the app had turned On are Off the next time the list is read, and the ones they chose stay as they are

#### Scenario: A site approved later
- **WHEN** a site is approved after the person's list was first read
- **THEN** the next reading sets it On or Off like the others

### Requirement: The list is grouped by what the sites offer
The list SHALL be in the groups Remote, worldwide; Europe; the person's country; By trade; Startups
and all-jobs sites, in that order, a group with no site left out. A site goes in the first group
that fits: By trade when it is built around a stack or lists one or two kinds of jobs none of them
tech; Startups and all-jobs when it lists startups; the person's country; Europe; Remote, worldwide
when it is remote-first and open worldwide; else Startups and all-jobs. Each site SHALL say what it
offers (its kinds of jobs, where, how remote, its speciality), its typical salary when known (the
average of the salaries its jobs state when at least ten state one, else its country's typical pay),
whether it is On, and whether it needs a key. The answer SHALL give how many sites are available and
how many are On, whatever is searched or shown. A site is found by part of its name, and the list
shows all sites, the ones that are On, remote ones, European ones, the person's country's or By trade.

#### Scenario: Find a site
- **WHEN** the person types "rub" in Find a site
- **THEN** only the sites with "rub" in their name are listed, and the counts stay those of the whole list

#### Scenario: Show the sites that are On
- **WHEN** the person shows the sites that are On
- **THEN** only those are listed, in their groups

### Requirement: Sites that do not allow apps are one sentence, not rows
The list SHALL not show a site whose terms or robots rules forbid an app reading it, or that needs
written permission first, as a row. It SHALL give their count and their names (the first ones), for
one sentence under the list, and SHALL give how many company hiring pages the app also reads.

#### Scenario: A site whose terms forbid apps
- **WHEN** a waiting site's terms forbid automated reading
- **THEN** it is not a row, and the sentence under the list names it and counts it

### Requirement: Company suggestions can be taken back
A person who said "Not interested" in a suggested company SHALL be able to take it back at once, and
the company SHALL be a suggestion again; for a company they never dropped the answer SHALL be not
found. Following a company again after stopping it is watching it again.

#### Scenario: Drop, then Undo
- **WHEN** a person drops a suggested company and then undoes it
- **THEN** the company is suggested again with its reasons

### Requirement: The Job sites page is one list for everyone, and the owners' view stays one tap away
The Job sites page SHALL show the person's list: a count line ("N available, M on"), a "Find a site"
box and one "Show" select, the groups with a name, one plain line, the typical salary and a single
On or Off button per site, "Add the key" for a site that needs one (the owner goes to the key, anyone
else is told the admin adds it), the sentence about sites that do not allow apps, a Companies section
and the count of company pages. Keep follows a company, and its row then reads "Following" with Stop;
every action shows Undo in the row and no row vanishes. The board owners SHALL also see "All sites",
the admin view, and it SHALL open first for them.

#### Scenario: Turn a site On
- **WHEN** a person presses Off on a site
- **THEN** the button reads On, the count of sites that are On goes up by one, and the choice is kept

#### Scenario: Keep a company
- **WHEN** a person presses Keep on a suggested company
- **THEN** its row reads "Following" with Stop and Undo is offered; the row does not leave the list

#### Scenario: An owner
- **WHEN** a board owner opens the Job sites page
- **THEN** "All sites" is open, with "Your sites" beside it
