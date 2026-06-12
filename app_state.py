'''
Shared model state for the GUI.

`AppState` owns the read-only reference data used across every tab (gear lists,
weaponskill tables, job/spell maps, optimizer ring filters). It holds no Qt
widgets so it can be constructed and inspected without a running application.
'''

import numpy as np

import gear as gear_pyfile


class AppState:
    '''Read-only reference data shared across all GUI tabs.'''

    def __init__(self):
        item_tmp = np.loadtxt("item_list.csv", delimiter=";", skiprows=1, dtype=str, unpack=True)
        self.item_id_dict = {"id": item_tmp[0], "name": item_tmp[1], "name2": item_tmp[2]}

        self.all_equipment_dict = {
            "main": gear_pyfile.mains,
            "sub": gear_pyfile.subs + gear_pyfile.grips,
            "ranged": gear_pyfile.ranged,
            "ammo": gear_pyfile.ammos,
            "head": gear_pyfile.heads,
            "neck": gear_pyfile.necks,
            "ear1": gear_pyfile.ears,
            "ear2": gear_pyfile.ears2,
            "body": gear_pyfile.bodies,
            "hands": gear_pyfile.hands,
            "ring1": gear_pyfile.rings,
            "ring2": gear_pyfile.rings2,
            "back": gear_pyfile.capes,
            "waist": gear_pyfile.waists,
            "legs": gear_pyfile.legs,
            "feet": gear_pyfile.feet,
            }

        self.ws_dict = {
            "Katana": ["Blade: Retsu", "Blade: Teki", "Blade: To", "Blade: Chi", "Blade: Ei", "Blade: Jin", "Blade: Ten", "Blade: Ku", "Blade: Yu", "Blade: Metsu", "Blade: Kamu", "Blade: Hi", "Blade: Shun", "Zesho Meppo",],
            "Great Katana": ["Tachi: Enpi", "Tachi: Goten", "Tachi: Kagero", "Tachi: Jinpu", "Tachi: Koki", "Tachi: Yukikaze", "Tachi: Gekko", "Tachi: Kasha", "Tachi: Ageha", "Tachi: Kaiten", "Tachi: Rana", "Tachi: Fudo", "Tachi: Shoha", "Tachi: Mumei",],
            "Dagger": [ "Viper Bite", "Dancing Edge", "Shark Bite", "Evisceration", "Aeolian Edge", "Mercy Stroke", "Mandalic Stab", "Mordant Rime", "Pyrrhic Kleos", "Rudra's Storm", "Exenterator", "Ruthless Stroke",],
            "Sword": ["Fast Blade", "Fast Blade II", "Burning Blade", "Red Lotus Blade", "Seraph Blade", "Circle Blade", "Swift Blade", "Savage Blade", "Sanguine Blade", "Knights of Round", "Death Blossom", "Expiacion", "Chant du Cygne", "Requiescat", "Imperator",],
            "Scythe": ["Slice", "Dark Harvest", "Shadow of Death", "Nightmare Scythe", "Spinning Scythe", "Guillotine", "Cross Reaper", "Spiral Hell", "Infernal Scythe", "Catastrophe", "Quietus", "Insurgency", "Entropy", "Origin",],
            "Great Sword": ["Hard Slash", "Freezebite", "Shockwave", "Sickle Moon", "Spinning Slash", "Ground Strike", "Herculean Slash", "Resolution", "Scourge", "Dimidiation", "Torcleaver", "Fimbulvetr",],
            "Club": ["Shining Strike", "Seraph Strike", "Skullbreaker", "True Strike", "Judgment", "Hexa Strike", "Black Halo", "Randgrith", "Exudation", "Mystic Boon", "Realmrazer", "Dagda",],
            "Polearm": ["Double Thrust", "Thunder Thrust", "Raiden Thrust", "Penta Thrust", "Wheeling Thrust", "Impulse Drive", "Sonic Thrust", "Geirskogul", "Drakesbane", "Camlann's Torment", "Stardiver", "Diarmuid",],
            "Staff": ["Heavy Swing", "Rock Crusher", "Earth Crusher", "Starburst", "Sunburst", "Shell Crusher", "Full Swing", "Cataclysm", "Retribution", "Gate of Tartarus", "Omniscience", "Vidohunir", "Garland of Bliss", "Shattersoul", "Oshala",],
            "Great Axe": ["Iron Tempest", "Shield Break", "Armor Break", "Weapon Break", "Raging Rush", "Full Break", "Steel Cyclone", "Fell Cleave", "Metatron Torment", "King's Justice", "Ukko's Fury", "Upheaval", "Disaster",],
            "Axe": ["Raging Axe", "Spinning Axe", "Rampage", "Calamity", "Mistral Axe", "Decimation", "Bora Axe", "Onslaught", "Primal Rend", "Cloudsplitter", "Ruinator", "Blitz",],
            "Archery": ["Flaming Arrow", "Piercing Arrow", "Dulling Arrow", "Sidewinder", "Blast Arrow", "Empyreal Arrow", "Refulgent Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Sarv",],
            "Marksmanship": ["Hot Shot", "Split Shot", "Sniper Shot", "Slug Shot", "Blast Shot", "Detonator", "Coronach", "Leaden Salute", "Trueflight", "Wildfire", "Last Stand", "Terminus",],
            "Hand-to-Hand": ["Combo", "One Inch Punch", "Raging Fists", "Spinning Attack", "Howling Fist", "Dragon Kick", "Asuran Fists", "Tornado Kick", "Ascetic's Fury", "Stringing Pummel", "Final Heaven", "Victory Smite", "Shijin Spiral", "Maru Kala", "Dragon Blow",],
            "None": ["None"],
            }

        self.jobs_dict = {"Ninja":"nin", "Dark Knight":"drk", "Scholar":"sch", "Red Mage":"rdm", "Black Mage":"blm", "Samurai":"sam", "Dragoon":"drg", "White Mage":"whm", "Warrior":"war", "Corsair":"cor", "Bard":"brd", "Thief":"thf", "Monk":"mnk", "Dancer":"dnc", "Beastmaster":"bst", "Rune Fencer":"run", "Ranger":"rng", "Puppetmaster":"pup", "Blue Mage":"blu", "Geomancer":"geo", "Paladin":"pld", "Summoner":"smn"}

        self.rema_weapons = [
                        "Amanomurakumo", "Annihilator", "Apocalypse", "Bravura", "Excalibur", "Gungnir", "Guttler", "Kikoku", "Mandau", "Mjollnir", "Ragnarok", "Spharai", "Yoichinoyumi",
                        "Almace", "Armageddon", "Caladbolg", "Farsha", "Gandiva", "Kannagi", "Masamune", "Redemption", "Rhongomiant", "Twashtar", "Ukonvasara", "Verethragna", "Hvergelmir",
                        "Aymur", "Burtgang", "Carnwenhan", "Conqueror", "Death Penalty", "Gastraphetes", "Glanzfaust", "Kenkonken", "Kogarasumaru", "Laevateinn", "Liberator", "Murgleis", "Nagi", "Ryunohige", "Terpsichore", "Tizona", "Tupsimati", "Nirvana", "Vajra", "Yagrush",
                        "Epeolatry", "Idris",
                        "Aeneas", "Anguta", "Chango", "Dojikiri Yasutsuna", "Fail-not", "Fomalhaut", "Godhands", "Heishi Shorinken", "Khatvanga", "Lionheart", "Sequence", "Tishtrya", "Tri-edge", "Trishula",
                        ]

        self.spells_dict = {
                    "nin":["Doton: Ichi", "Doton: Ni", "Doton: San",
                          "Suiton: Ichi", "Suiton: Ni", "Suiton: San",
                          "Huton: Ichi", "Huton: Ni", "Huton: San",
                          "Katon: Ichi", "Katon: Ni", "Katon: San",
                          "Hyoton: Ichi", "Hyoton: Ni", "Hyoton: San",
                          "Raiton: Ichi", "Raiton: Ni", "Raiton: San",
                          "Ranged Attack"],
                    "blm":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V", "Stone VI", "Stoneja",
                           "Water", "Water II", "Water III", "Water IV", "Water V", "Water VI", "Waterja",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V", "Aero VI", "Aeroja",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V", "Fire VI", "Firaja",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V", "Blizzard VI", "Blizzaja",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Thunder VI", "Thundaja", "Impact",
                           "Ranged Attack"],
                    "rdm":["EnSpell",
                           "Stone", "Stone II", "Stone III", "Stone IV", "Stone V",
                           "Water", "Water II", "Water III", "Water IV", "Water V",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Impact", "Ranged Attack"],
                    "geo":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V",
                           "Water", "Water II", "Water III", "Water IV", "Water V",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Impact"],
                    "sch":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V", "Geohelix II",
                           "Water", "Water II", "Water III", "Water IV", "Water V", "Hydrohelix II",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V", "Anemohelix II",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V", "Pyrohelix II",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V", "Cryohelix II",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Ionohelix II",
                           "Luminohelix II", "Noctohelix II", "Kaustra", "Impact",],
                    "drk":["Stone", "Stone II", "Stone III",
                           "Water", "Water II", "Water III",
                           "Aero", "Aero II", "Aero III",
                           "Fire", "Fire II", "Fire III",
                           "Blizzard", "Blizzard II", "Blizzard III",
                           "Thunder", "Thunder II", "Thunder III", "Impact"],
                    "cor":["Ranged Attack", "Earth Shot", "Water Shot", "Wind Shot", "Fire Shot", "Ice Shot", "Thunder Shot"],
                    "rng":["Ranged Attack"],
                    "sam":["Ranged Attack"],
                    "thf":["Ranged Attack"],
                    }

        self.equipment_button_positions = {
            "main":  [0,0],
            "sub":   [0,1],
            "ranged":[0,2],
            "ammo":  [0,3],
            "head":  [1,0],
            "neck":  [1,1],
            "ear1":  [1,2],
            "ear2":  [1,3],
            "body":  [2,0],
            "hands": [2,1],
            "ring1": [2,2],
            "ring2": [2,3],
            "back":  [3,0],
            "waist": [3,1],
            "legs":  [3,2],
            "feet":  [3,3]
        }

        self.tvr_rings = ["Cornelia's", "Ephramad's", "Fickblix's", "Gurebu-Ogurebu's", "Lehko Habhoka's", "Medada's", "Ragelise's", "None"]
        self.soa_rings = ["Weatherspoon", "Karieyh", "Vocane", "None"]
