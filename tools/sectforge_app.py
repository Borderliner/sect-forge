"""Sect Forge desktop editor."""

from concurrent.futures import ThreadPoolExecutor
import math
import os
from pathlib import Path
import queue
import re
import tkinter as tk
from tkinter import colorchooser, filedialog, messagebox

from sectforge import BASE_ART, OUTPUT, Rpfm, build, portrait_catalog, settings


BACKGROUND = "#171918"
PANEL = "#222522"
FIELD = "#303530"
INK = "#f1eee5"
MUTED = "#adb6ac"
GOLD = "#e4bf76"
ERROR = "#ffada2"
FEATURED_PORTRAIT = "3k_main_art_set_historical_zhao_yun_general"


def label(parent, text, *, color=INK, size=10, bold=False, background=PANEL, **options):
    return tk.Label(parent, text=text, bg=background, fg=color,
                    font=("Segoe UI", size, "bold" if bold else "normal"), **options)


def button(parent, text, command, *, primary=False, pady=8, **options):
    return tk.Button(parent, text=text, command=command, bg=GOLD if primary else FIELD,
                     fg=BACKGROUND if primary else INK, activebackground="#f2d49c" if primary else "#424b43",
                     activeforeground=BACKGROUND if primary else INK, disabledforeground=MUTED,
                     borderwidth=0, padx=15, pady=pady, font=("Segoe UI", 10, "bold"), **options)


class SectForge(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Sect Forge")
        self.geometry("1030x715")
        self.minsize(900, 700)
        self.configure(bg=BACKGROUND)
        self.worker = ThreadPoolExecutor(max_workers=1)
        self.pending = []
        self.messages = queue.SimpleQueue()
        self.busy = False
        self.source = None
        self.vanilla = None
        self.portraits = {}
        self.visible = []
        self.selected = None
        self.preview_id = 0
        self.image = None
        self.output = None
        self.values = {key: tk.StringVar(value=value) for key, value in {
            "sectName": "Way of the White Lotus", "forename": "Wei", "surname": "Yun",
            "primaryColour": "6A0DAD", "secondaryColour": "E0C060",
        }.items()}
        self.search = tk.StringVar()
        self.status = tk.StringVar(value="Loading the portrait library…")
        self.create_layout()
        self.protocol("WM_DELETE_WINDOW", self.close)
        self.after(100, self.poll)
        self.load_catalog(None)

    def create_layout(self):
        header = tk.Frame(self, bg=BACKGROUND)
        header.pack(fill="x", padx=32, pady=(23, 19))
        label(header, "TOTAL WAR  /  THREE KINGDOMS", color=GOLD, bold=True, background=BACKGROUND).pack(anchor="w")
        label(header, "Sect Forge", size=29, bold=True, background=BACKGROUND).pack(anchor="w")
        label(header, "Found a faction for the 190 CE campaign.", color=MUTED, size=11,
              background=BACKGROUND).pack(anchor="w", pady=(1, 0))

        body = tk.Frame(self, bg=BACKGROUND)
        body.pack(fill="both", expand=True, padx=32)
        body.columnconfigure(0, weight=3, uniform="workspace")
        body.columnconfigure(1, weight=2, uniform="workspace")
        body.rowconfigure(0, weight=1)
        editor = tk.Frame(body, bg=PANEL, padx=22, pady=15)
        editor.grid(row=0, column=0, sticky="nsew", padx=(0, 12))
        editor.columnconfigure((0, 1), weight=1, uniform="field")
        editor.rowconfigure(9, weight=1)

        label(editor, "01   FACTION IDENTITY", color=GOLD, bold=True).grid(row=0, column=0, columnspan=2, sticky="w")
        self.field(editor, "Faction name", "sectName", 1, 0, 2)
        self.field(editor, "Primary colour", "primaryColour", 3, 0, colour=True)
        self.field(editor, "Secondary colour", "secondaryColour", 3, 1, colour=True)
        label(editor, "02   FOUNDING LEADER", color=GOLD, bold=True).grid(row=5, column=0, columnspan=2, sticky="w", pady=(16, 0))
        self.field(editor, "Family name", "surname", 6, 0)
        self.field(editor, "Given name", "forename", 6, 1)

        bar = tk.Frame(editor, bg=PANEL)
        bar.grid(row=8, column=0, columnspan=2, sticky="ew", pady=(12, 6))
        self.entry(bar, self.search).pack(side="left", fill="x", expand=True, ipady=6)
        self.search.trace_add("write", lambda *_: self.filter_portraits())
        self.import_button = button(bar, "Browse mod…", self.choose_mod)
        self.import_button.pack(side="left", padx=(8, 0))
        self.vanilla_button = button(bar, "Vanilla", lambda: self.load_catalog(None))
        self.vanilla_button.pack(side="left", padx=(6, 0))

        list_frame = tk.Frame(editor, bg=FIELD)
        list_frame.grid(row=9, column=0, columnspan=2, sticky="nsew")
        self.listbox = tk.Listbox(list_frame, height=4, bg=FIELD, fg=INK, selectbackground=GOLD,
                                  selectforeground=BACKGROUND, highlightthickness=1, highlightcolor=GOLD,
                                  highlightbackground=FIELD, borderwidth=0, font=("Segoe UI", 10), exportselection=False)
        scrollbar = tk.Scrollbar(list_frame, command=self.listbox.yview)
        self.listbox.configure(yscrollcommand=scrollbar.set)
        scrollbar.pack(side="right", fill="y")
        self.listbox.pack(fill="both", expand=True)
        self.listbox.bind("<<ListboxSelect>>", self.choose_portrait)
        self.library = label(editor, "", color=MUTED, size=9, anchor="w")
        self.library.grid(row=10, column=0, columnspan=2, sticky="ew", pady=(4, 0))

        preview = tk.Frame(body, bg=PANEL, padx=22, pady=15)
        preview.grid(row=0, column=1, sticky="nsew")
        label(preview, "CAMPAIGN PREVIEW", color=GOLD, bold=True).pack(anchor="w")
        self.art = tk.Label(preview, bg=BACKGROUND, fg=MUTED, font=("Segoe UI", 10), text="Select a portrait")
        self.art.pack(fill="both", expand=True, pady=(10, 9))
        stripe = tk.Frame(preview, bg=PANEL)
        stripe.pack(fill="x")
        self.swatches = [tk.Frame(stripe, height=5), tk.Frame(stripe, height=5)]
        for swatch in self.swatches:
            swatch.pack(side="left", fill="x", expand=True)
        self.faction = label(preview, "", size=15, bold=True, wraplength=280, justify="center")
        self.faction.pack(fill="x", pady=(8, 0))
        self.leader = label(preview, "", color=GOLD, size=11, wraplength=280, justify="center")
        self.leader.pack(fill="x", pady=(1, 4))
        label(preview, "HEPU  /  190 CE  /  ROMANCE + RECORDS", color=MUTED, size=9).pack()

        footer = tk.Frame(self, bg=BACKGROUND)
        footer.pack(fill="x", padx=32, pady=(14, 19))
        self.status_label = label(footer, "", textvariable=self.status, color=MUTED, size=9,
                                  background=BACKGROUND, anchor="w", justify="left", wraplength=520)
        self.status_label.pack(side="left", fill="x", expand=True, padx=(0, 8))
        self.build_button = button(footer, "Build mod pack", self.export, primary=True, state="disabled")
        self.build_button.pack(side="right")
        self.folder_button = button(footer, "Open output folder", self.open_output, state="disabled")
        self.folder_button.pack(side="right", padx=(0, 8))
        for value in self.values.values():
            value.trace_add("write", lambda *_: self.update_identity())
        self.update_identity()

    def entry(self, parent, value):
        return tk.Entry(parent, textvariable=value, bg=FIELD, fg=INK, insertbackground=INK,
                        borderwidth=0, highlightthickness=1, highlightbackground=FIELD,
                        highlightcolor=GOLD, font=("Segoe UI", 11))

    def field(self, parent, title, key, row, column, span=1, colour=False):
        padding = (0, 8) if column == 0 and span == 1 else 0
        label(parent, title, color=MUTED, size=9).grid(row=row, column=column, sticky="w", pady=(5, 2))
        field = tk.Frame(parent, bg=PANEL)
        field.grid(row=row + 1, column=column, columnspan=span, sticky="ew", padx=padding)
        self.entry(field, self.values[key]).pack(side="left", fill="x", expand=True, ipady=5)
        if colour:
            button(field, "Pick", lambda: self.pick_colour(key), pady=4).pack(side="right", padx=(5, 0))

    def pick_colour(self, key):
        current = self.values[key].get().strip().lstrip("#")
        initial = "#" + current if re.fullmatch(r"[0-9a-fA-F]{6}", current) else GOLD
        _, chosen = colorchooser.askcolor(initial, parent=self, title="Choose faction colour")
        if chosen:
            self.values[key].set(chosen[1:].upper())

    def update_identity(self):
        self.faction.configure(text=self.values["sectName"].get().strip() or "Unnamed faction")
        name = " ".join(self.values[key].get().strip() for key in ("surname", "forename")).strip()
        self.leader.configure(text=name or "Unnamed leader")
        for swatch, key in zip(self.swatches, ("primaryColour", "secondaryColour")):
            colour = self.values[key].get().strip().lstrip("#")
            swatch.configure(bg="#" + colour if re.fullmatch(r"[0-9a-fA-F]{6}", colour) else FIELD)

    def set_status(self, message, error=False):
        self.status.set(message)
        self.status_label.configure(fg=ERROR if error else MUTED)

    def set_busy(self, value):
        self.busy = value
        for item in (self.import_button, self.vanilla_button):
            item.configure(state="disabled" if value else "normal")
        self.build_button.configure(state="normal" if self.selected and not value else "disabled")

    def submit(self, operation, success, failure):
        self.pending.append((self.worker.submit(operation), success, failure))

    def poll(self):
        while not self.messages.empty():
            self.set_status(self.messages.get())
        for entry in self.pending[:]:
            future, success, failure = entry
            if future.done():
                self.pending.remove(entry)
                try:
                    result = future.result()
                except Exception as exc:
                    failure(str(exc))
                else:
                    success(result)
        self.after(80, self.poll)

    def choose_mod(self):
        chosen = filedialog.askopenfilename(parent=self, title="Select a portrait mod", filetypes=[("Total War pack", "*.pack")])
        if chosen:
            self.load_catalog(Path(chosen))

    def load_catalog(self, source):
        def load():
            api = Rpfm()
            if source:
                with api.pack(path=source) as mod:
                    portraits, vanilla = portrait_catalog(api, mod, self.vanilla)
                portraits = {key: portrait for key, portrait in portraits.items() if portrait["from_mod"]}
                if not portraits:
                    raise ValueError(f"No compatible adult portraits found in {source.name}")
                return portraits, vanilla
            return portrait_catalog(api, vanilla=self.vanilla)

        def loaded(result):
            self.portraits, self.vanilla = result
            self.source = source
            self.selected = next((key for key in (FEATURED_PORTRAIT, BASE_ART) if key in self.portraits), None)
            self.search.set("")
            self.filter_portraits()
            self.set_busy(False)
            self.set_status("Ready to build  •  Male Wood-class leader  •  One generated pack per campaign")

        self.set_busy(True)
        self.set_status(f"Reading {source.name if source else 'vanilla portraits'}…")
        self.submit(load, loaded, self.failed)

    def filter_portraits(self):
        term = self.search.get().strip().casefold()
        self.visible = sorted((key for key, portrait in self.portraits.items()
                               if term in (portrait["label"] + " " + key).casefold()),
                              key=lambda key: (self.portraits[key]["label"].casefold(), key))
        self.listbox.delete(0, tk.END)
        for key in self.visible:
            self.listbox.insert(tk.END, self.portraits[key]["label"])
        self.library.configure(text=f"{len(self.visible)} portraits  •  {self.source.name if self.source else 'Vanilla'}  •  Search by name or art key")
        if self.visible:
            index = self.visible.index(self.selected) if self.selected in self.visible else 0
            self.listbox.selection_set(index)
            self.listbox.see(index)
            self.choose_portrait()
        else:
            self.selected = None
            self.preview_id += 1
            self.image = None
            self.art.configure(image="", text="No matching portraits")
        if not self.busy:
            self.set_busy(False)

    def choose_portrait(self, _event=None):
        indexes = self.listbox.curselection()
        if not indexes:
            return
        key = self.visible[indexes[0]]
        self.selected = key
        self.preview_id += 1
        request_id = self.preview_id
        source, portrait = self.source, self.portraits[key]
        self.image = None
        self.art.configure(image="", text=portrait["label"])
        if not self.busy:
            self.set_busy(False)

        def preview():
            if request_id != self.preview_id:
                return None
            api = Rpfm()
            if source and portrait["override"]:
                with api.pack(path=source) as mod:
                    return api.raw(mod, portrait["override"])
            for path in portrait["paths"]:
                if path in api.files("game_files", path):
                    return api.raw("game_files", path)
            return None

        def show(data):
            if request_id == self.preview_id and data:
                try:
                    image = tk.PhotoImage(data=data)
                    scale = max(1, math.ceil(max(image.width() / 270, image.height() / 280)))
                    if scale > 1:
                        image = image.subsample(scale)
                    elif max(image.width(), image.height()) < 135:
                        image = image.zoom(2)
                    self.image = image
                    self.art.configure(image=self.image, text="")
                except tk.TclError:
                    self.art.configure(text=portrait["label"] + "\nPreview unavailable")

        self.submit(preview, show, lambda _: show(None))

    def export(self):
        config = {key: value.get() for key, value in self.values.items()}
        config.update(artSet=self.selected, portraitPack=str(self.source) if self.source else None)
        try:
            config = settings(config)
        except ValueError as exc:
            self.set_status(str(exc), error=True)
            return
        target = OUTPUT / f"{config['factionKey']}.pack"
        if target.exists() and not messagebox.askyesno("Replace generated pack", f"Replace {target.name}?", parent=self):
            return
        self.set_busy(True)
        self.set_status("Building your campaign…")
        self.submit(lambda: build(config, progress=self.messages.put), self.built, self.failed)

    def built(self, target):
        self.output = target.parent
        self.set_busy(False)
        self.folder_button.configure(state="normal")
        self.set_status(f"Exported {target.name}. Enable it in your mod manager for a new 190 campaign.")

    def failed(self, message):
        self.set_busy(False)
        self.set_status(message, error=True)

    def open_output(self):
        if self.output:
            os.startfile(self.output)

    def close(self):
        if self.busy:
            self.set_status("The current operation is finishing…")
            return
        self.worker.shutdown(wait=False, cancel_futures=True)
        self.destroy()


if __name__ == "__main__":
    SectForge().mainloop()
