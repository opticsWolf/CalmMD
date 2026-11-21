//! Application labels are separate from per-document spelling dictionaries.
use gpui_kit::App;

pub const KEY: &str = "gpui/interface-language";

pub fn tr<'a>(_: &App, text: &'a str) -> &'a str {
    translate(text)
}

pub fn translate(text: &str) -> &str {
    if &*gpui_kit::component::locale() == "de" {
        german(text)
    } else {
        text
    }
}

pub fn message(cx: &App, text: &str) -> String {
    let translated = tr(cx, text);
    if translated != text {
        return translated.to_owned();
    }
    if &*gpui_kit::component::locale() == "de" {
        for prefix in [
            "Folder opened: ",
            "Could not load recovery drafts: ",
            "Cannot open draft: ",
            "Could not save: ",
            "Saved a copy to ",
            "Space is open in ",
            "Could not rotate the MCP token: ",
            "Enter a line number from 1 to ",
            "Linked note not found: ",
            "Could not start MCP: ",
            "Workspace changed: ",
            "Export failed: ",
            "External link: ",
            "Opened Space: ",
            "Space saved: ",
            "Exported ",
            "Anchor: ",
            "Recover ",
            "Restore ",
            "Opened ",
            "Remove ",
            "Open ",
        ] {
            if let Some(rest) = text.strip_prefix(prefix) {
                return format!("{}{rest}", german(prefix));
            }
        }
    }
    text.to_owned()
}

fn german(text: &str) -> &str {
    match text {
        "Mermaid source for this diagram" => "Mermaid-Quelltext für dieses Diagramm",
        "Add Nodes" => "Knoten hinzufügen",
        "Add Participants" => "Teilnehmer hinzufügen",
        "Edit Connections" => "Verbindungen bearbeiten",
        "Existing Connections" => "Bestehende Verbindungen",
        "Node ID" => "Knoten-ID",
        "Label (optional)" => "Beschriftung (optional)",
        "Connection label (optional)" => "Verbindungsbeschriftung (optional)",
        "Add Row" => "Zeile hinzufügen",
        "Remove Row" => "Zeile entfernen",
        "Node IDs must be unique." => "Knoten-IDs müssen eindeutig sein.",
        "Enter at least one node ID." => "Geben Sie mindestens eine Knoten-ID ein.",
        "Select two existing nodes." => "Wählen Sie zwei vorhandene Knoten aus.",
        "Select a connection." => "Wählen Sie eine Verbindung aus.",
        "The current diagram cannot be edited in this dialog." => "Dieses Diagramm kann in diesem Dialog nicht bearbeitet werden.",
        "Connect" => "Verbinden",
        "Disconnect" => "Verbindung trennen",
        "Reconnect" => "Neu verbinden",
        "From" => "Von",
        "To" => "Nach",
        "Add Folder" => "Ordner hinzufügen",
        "Add to Favourites" => "Zu Favoriten hinzufügen",
        "Remove from Favourites" => "Aus Favoriten entfernen",
        "This favourite is unavailable." => "Dieser Favorit ist nicht verfügbar.",
        "Hide Sidebar" => "Seitenleiste ausblenden",
        "Show Sidebar" => "Seitenleiste anzeigen",
        "New Folder" => "Neuer Ordner",
        "Move to Folder" => "In Ordner verschieben",
        "Destination folder" => "Zielordner",
        "Name" => "Name",
        "Rename" => "Umbenennen",
        "Duplicate" => "Duplizieren",
        "Copy Path" => "Pfad kopieren",
        "Move to Trash" => "In den Papierkorb verschieben",
        "Open" => "Öffnen",
        "Add Node" => "Knoten hinzufügen",
        "Connect Nodes" => "Knoten verbinden",
        "Add Subgraph" => "Teilgraph hinzufügen",
        "Diagram Direction" => "Diagrammrichtung",
        "Add Participant" => "Teilnehmer hinzufügen",
        "Add Message" => "Nachricht hinzufügen",
        "Add Comment" => "Kommentar hinzufügen",
        "ID or ID: label" => "ID oder ID: Beschriftung",
        "Source and target IDs" => "Quell- und Ziel-ID",
        "Sender and receiver IDs, then : message" => "Absender- und Empfänger-ID, danach : Nachricht",
        "Comment text" => "Kommentartext",
        "Direction (TD, TB, BT, LR, RL)" => "Richtung (TD, TB, BT, LR, RL)",
        "Enter a value on one line." => "Geben Sie einen einzeiligen Wert ein.",
        "Choose TD, TB, BT, LR, or RL." => "Wählen Sie TD, TB, BT, LR oder RL.",
        "Use letters, digits, or underscores in Mermaid IDs." => "Verwenden Sie in Mermaid-IDs Buchstaben, Ziffern oder Unterstriche.",
        "This diagram has no flowchart header." => "Dieses Diagramm hat keinen Flussdiagrammkopf.",
        "Line Numbers" => "Zeilennummern",
        "Could not open folder" => "Ordner konnte nicht ge\u{00f6}ffnet werden",
        "The selected folder is unavailable." => "Der ausgew\u{00e4}hlte Ordner ist nicht verf\u{00fc}gbar.",

        "Recovered draft — choose which version to keep" => "Entwurf wiederhergestellt – wählen Sie eine Version",
        "Draft has no parent folder" => "Der Entwurf hat keinen übergeordneten Ordner",
        "MCP server task stopped" => "MCP-Serverprozess wurde beendet",
        "HTML copied" => "HTML kopiert",
        "Choose an export destination different from the source document." => "Wählen Sie für den Export einen anderen Pfad als den des Quelldokuments.",
        "Could not load recovery drafts: " => "Entwürfe konnten nicht geladen werden: ",
        "Cannot open draft: " => "Entwurf kann nicht geöffnet werden: ",
        "Could not save: " => "Speichern fehlgeschlagen: ",
        "Saved a copy to " => "Kopie gespeichert unter: ",
        "Space is open in " => "Arbeitsbereich ist geöffnet in: ",
        "Select a folder" => "Ordner ausw\u{00e4}hlen",
        "Open a document" => "Dokument \u{00f6}ffnen",
        "Select an image" => "Bild ausw\u{00e4}hlen",
        "Image" => "Bild",
        "Save file" => "Datei speichern",
        "selected" => "ausgew\u{00e4}hlt",
        "File" => "Datei",
        "Edit" => "Bearbeiten",
        "View" => "Ansicht",
        "Insert" => "Einfügen",
        "Format" => "Format",
        "Tools" => "Werkzeuge",
        "Help" => "Hilfe",
        "New Window" => "Neues Fenster",
        "New Document" => "Neues Dokument",
        "Open…" => "Öffnen…",
        "Open Folder…" => "Ordner öffnen…",
        "Open Recent Folder" => "Zuletzt verwendete Ordner",
        "Open Parent Folder" => "Übergeordneten Ordner öffnen",
        "Spaces" => "Arbeitsbereiche",
        "Save" => "Speichern",
        "Save a Copy…" => "Kopie speichern…",
        "Save Version..." => "Version speichern…",
        "Save Version" => "Version speichern",
        "Version History" => "Versionsverlauf",
        "Close Document" => "Dokument schließen",
        "Reopen Closed Document" => "Geschlossenes Dokument erneut öffnen",
        "Close Folder" => "Ordner schließen",
        "Start MCP Server" => "MCP-Server starten",
        "Stop MCP Server" => "MCP-Server stoppen",
        "New MCP Token" => "Neues MCP-Token",
        "Reveal in File Manager" => "Im Dateimanager anzeigen",
        "Quit" => "Beenden",
        "Undo" => "Rückgängig",
        "Redo" => "Wiederholen",
        "Cut" => "Ausschneiden",
        "Copy" => "Kopieren",
        "Paste" => "Einfügen",
        "Select All" => "Alles auswählen",
        "Find in Document" => "Im Dokument suchen",
        "Go to Line..." => "Zu Zeile springen…",
        "Go to Line" => "Zu Zeile springen",
        "Search in Folder" => "Im Ordner suchen",
        "Copy Markdown" => "Markdown kopieren",
        "Copy HTML" => "HTML kopieren",
        "Source" => "Quelltext",
        "Live" => "Live",
        "Split" => "Geteilt",
        "Preview" => "Vorschau",
        "Files" => "Dateien",
        "Outline" => "Gliederung",
        "Search" => "Suche",
        "History" => "Verlauf",
        "Appearance..." => "Darstellung…",
        "Appearance" => "Darstellung",
        "Toggle Sidebar" => "Seitenleiste ein-/ausblenden",
        "Toggle Toolbar" => "Symbolleiste ein-/ausblenden",
        "Customize Toolbar..." => "Symbolleiste anpassen…",
        "Customize Toolbar" => "Symbolleiste anpassen",
        "Zoom In" => "Vergrößern",
        "Zoom Out" => "Verkleinern",
        "Reset Zoom" => "Zoom zurücksetzen",
        "Next Window" => "Nächstes Fenster",
        "Close Window" => "Fenster schließen",
        "Link..." => "Link…",
        "Link" => "Link",
        "Table..." => "Tabelle…",
        "Table" => "Tabelle",
        "Code Block" => "Codeblock",
        "Math Block" => "Formelblock",
        "Math" => "Formel",
        "Image File..." => "Bilddatei…",
        "Image File" => "Bilddatei",
        "Image Link..." => "Bildlink…",
        "Image Asset Location..." => "Speicherort für Bilder…",
        "Image Asset Location" => "Speicherort für Bilder",
        "Mermaid Diagram" => "Mermaid-Diagramm",
        "Footnote..." => "Fußnote…",
        "Horizontal Rule" => "Trennlinie",
        "Bold" => "Fett",
        "Italic" => "Kursiv",
        "Strikethrough" => "Durchgestrichen",
        "Inline Code" => "Inline-Code",
        "Highlight" => "Hervorheben",
        "Heading" => "Überschrift",
        "Heading Level..." => "Überschriftenebene…",
        "Heading Level" => "Überschriftenebene",
        "Bullet List" => "Aufzählung",
        "Ordered List" => "Nummerierte Liste",
        "Task List" => "Aufgabenliste",
        "Quote" => "Zitat",
        "Spelling Language..." => "Rechtschreibsprache…",
        "Spelling Language" => "Rechtschreibsprache",
        "Interface Language..." => "Oberflächensprache…",
        "Interface Language" => "Oberflächensprache",
        "Command Palette..." => "Befehlspalette…",
        "Command Palette" => "Befehlspalette",
        "Go to File..." => "Zu Datei wechseln…",
        "Go to File" => "Zu Datei wechseln",
        "Go to Heading..." => "Zu Überschrift springen…",
        "Go to Heading" => "Zu Überschrift springen",
        "Check Spelling" => "Rechtschreibung prüfen",
        "Keyboard Shortcuts" => "Tastenkürzel",
        "Welcome" => "Willkommen",
        "Welcome to CalmMD" => "Willkommen bei CalmMD",
        "Markdown Guide" => "Markdown-Anleitung",
        "About CalmMD" => "Über CalmMD",
        "Export" => "Exportieren",
        "PDF..." => "PDF…",
        "HTML..." => "HTML…",
        "Open in New Tab" => "In neuem Tab öffnen",
        "New Tab" => "Neuer Tab",
        "Close Tab" => "Tab schließen",
        "Cancel" => "Abbrechen",
        "Done" => "Fertig",
        "Edit block" => "Abschnitt bearbeiten",
        "Add paragraph" => "Absatz hinzufügen",
        "Select a document, or create a new one." => {
            "Wählen Sie ein Dokument aus oder erstellen Sie ein neues."
        }
        "Open a folder to begin." => "Öffnen Sie zunächst einen Ordner.",
        "Find" => "Suchen",
        "Up" => "Nach oben",
        "Down" => "Nach unten",
        "Remove" => "Entfernen",
        "Add Separator" => "Trennzeichen hinzufügen",
        "Reset" => "Zurücksetzen",
        "Apply" => "Übernehmen",
        "Separator" => "Trennzeichen",
        "Restore" => "Wiederherstellen",
        "Restore version" => "Version wiederherstellen",
        "Saved version" => "Gespeicherte Version",
        "CalmMD will save the current text in history before restoring this version." => {
            "CalmMD nimmt den aktuellen Text in den Versionsverlauf auf, bevor diese Version wiederhergestellt wird."
        }
        "No saved versions" => "Keine gespeicherten Versionen",
        "No headings" => "Keine Überschriften",
        "No Space" => "Kein Arbeitsbereich",
        "No folder" => "Kein Ordner",
        "Open folder" => "Ordner öffnen",
        "FAVOURITES" => "FAVORITEN",
        "Star a file or folder to keep it here." => {
            "Markieren Sie eine Datei oder einen Ordner als Favorit."
        }
        "RECOVER DRAFTS" => "ENTWÜRFE WIEDERHERSTELLEN",
        "New" => "Neu",
        "Folder" => "Ordner",
        "Search notes" => "Notizen durchsuchen",
        "Spelling off" => "Rechtschreibprüfung aus",
        "Turn off spell check" => "Rechtschreibprüfung ausschalten",
        "Automatic (system languages)" => "Automatisch (Systemsprachen)",
        "Status" => "Status",
        "Changed on disk" => "Auf dem Datenträger geändert",
        "Edited" => "Bearbeitet",
        "Saving…" => "Wird gespeichert…",
        "Couldn’t save, retrying" => "Speichern fehlgeschlagen, erneuter Versuch",
        "Saved" => "Gespeichert",
        "Keep mine" => "Meine Version behalten",
        "Discard draft" => "Entwurf verwerfen",
        "Load theirs" => "Dateiversion laden",
        "Close" => "Schließen",
        "Filter" => "Filtern",
        "No matches" => "Keine Treffer",
        "Clear Recent Folders" => "Zuletzt verwendete Ordner löschen",
        "Save Current Space..." => "Aktuellen Arbeitsbereich speichern…",
        "Save Space" => "Arbeitsbereich speichern",
        "Space name" => "Name des Arbeitsbereichs",
        "Close Space" => "Arbeitsbereich schließen",
        "Remove Space" => "Arbeitsbereich entfernen",
        "Type the Space name to remove its saved workspace" => {
            "Geben Sie zur Bestätigung den Namen des Arbeitsbereichs ein"
        }
        "Light" => "Hell",
        "Dark" => "Dunkel",
        "Match System" => "Systemeinstellung verwenden",
        "Next to the document (assets/)" => "Neben dem Dokument (assets/)",
        "In the folder or Space root (assets/)" => "Im Hauptordner des Arbeitsbereichs (assets/)",
        "Export PDF" => "PDF exportieren",
        "Export HTML" => "HTML exportieren",
        "Export..." => "Exportieren…",
        "Cover author" => "Verfasser des Deckblatts",
        "Paper size" => "Papierformat",
        "Margins" => "Seitenränder",
        "Template" => "Vorlage",
        "Running header" => "Kopfzeile",
        "Page numbers" => "Seitenzahlen",
        "Start top-level headings on a new page" => "Neue Seite bei Überschriften der ersten Ebene",
        "Table of contents" => "Inhaltsverzeichnis",
        "Embed local images" => "Lokale Bilder einbetten",
        "Cover page" => "Deckblatt",
        "normal" => "normal",
        "narrow" => "schmal",
        "wide" => "breit",
        "standard" => "Standard",
        "report" => "Bericht",
        "academic" => "Wissenschaftlich",
        "minimal" => "minimal",
        "Insert Row Above" => "Zeile darüber einfügen",
        "Insert Row Below" => "Zeile darunter einfügen",
        "Insert Column Left" => "Spalte links einfügen",
        "Insert Column Right" => "Spalte rechts einfügen",
        "Delete Row" => "Zeile löschen",
        "Delete Column" => "Spalte löschen",
        "Align Column Left" => "Spalte linksbündig ausrichten",
        "Align Column Center" => "Spalte zentrieren",
        "Align Column Right" => "Spalte rechtsbündig ausrichten",
        "Format Table" => "Tabelle formatieren",
        "Insert Link" => "Link einfügen",
        "Link destination" => "Linkziel",
        "Insert Table" => "Tabelle einfügen",
        "Columns x rows" => "Spalten × Zeilen",
        "Version name (optional)" => "Versionsname (optional)",
        "Line number" => "Zeilennummer",
        "Insert Image Link" => "Bildlink einfügen",
        "Image URL or relative path" => "Bild-URL oder relativer Pfad",
        "Insert Footnote" => "Fußnote einfügen",
        "Footnote text" => "Fußnotentext",
        "Level 1 to 6" => "Ebene 1 bis 6",
        "Ready" => "Bereit",
        "Folder opened: " => "Ordner ge\u{00f6}ffnet: ",
        "Folder opened" => "Ordner geöffnet",
        "Folder closed" => "Ordner geschlossen",
        "Document created" => "Dokument erstellt",
        "Version saved" => "Version gespeichert",
        "Version restored" => "Version wiederhergestellt",
        "Autosaved" => "Automatisch gespeichert",
        "No document is open" => "Kein Dokument geöffnet",
        "MCP connection copied" => "MCP-Verbindungsdaten kopiert",
        "MCP server stopped" => "MCP-Server gestoppt",
        "New MCP token copied" => "Neues MCP-Token kopiert",
        "Start the MCP server before rotating its token" => {
            "Starten Sie den MCP-Server, bevor Sie sein Token erneuern"
        }
        "No recently closed document" => "Kein kürzlich geschlossenes Dokument",
        "The document has no path." => "Das Dokument hat keinen Pfad.",
        "The active document changed." => "Das aktive Dokument wurde gewechselt.",
        "The saved version is unavailable." => "Die gespeicherte Version ist nicht verfügbar.",
        "Open a folder before creating a document" => {
            "Öffnen Sie einen Ordner, bevor Sie ein Dokument erstellen"
        }
        "Open a document first." => "Öffnen Sie zuerst ein Dokument.",
        "Open a document before exporting." => "Öffnen Sie vor dem Export ein Dokument.",
        "Open a document before exporting" => "Öffnen Sie vor dem Export ein Dokument",
        "Open a document before inserting an image." => {
            "Öffnen Sie ein Dokument, bevor Sie ein Bild einfügen."
        }
        "Open a document before following a link" => {
            "Öffnen Sie ein Dokument, bevor Sie einem Link folgen"
        }
        "Place the cursor inside a Markdown table." => {
            "Setzen Sie den Cursor in eine Markdown-Tabelle."
        }
        "A Markdown table needs at least one column." => {
            "Eine Markdown-Tabelle benötigt mindestens eine Spalte."
        }
        "Enter a link destination." => "Geben Sie ein Linkziel ein.",
        "Enter an image URL or path." => "Geben Sie eine Bild-URL oder einen Pfad ein.",
        "Enter the footnote text." => "Geben Sie den Fußnotentext ein.",
        "Enter a heading level from 1 to 6." => {
            "Geben Sie eine Überschriftenebene von 1 bis 6 ein."
        }
        "Enter columns x rows, for example 3 x 2." => {
            "Geben Sie Spalten × Zeilen ein, zum Beispiel 3 × 2."
        }
        "Use a number from 1 to 30 for each dimension." => {
            "Geben Sie für beide Maße eine Zahl von 1 bis 30 ein."
        }
        "The document changed. Reopen this dialog to try again." => {
            "Das Dokument wurde geändert. Öffnen Sie diesen Dialog erneut."
        }
        "This folder is unavailable." => "Dieser Ordner ist nicht verfügbar.",
        "Recent folders cleared" => "Liste der zuletzt verwendeten Ordner gelöscht",
        "The Space name does not match." => "Der Name des Arbeitsbereichs stimmt nicht überein.",
        "Save conflict — choose a version" => "Speicherkonflikt – wählen Sie eine Version",
        "Document changed while editing — choose a version" => {
            "Das Dokument wurde während der Bearbeitung geändert. Wählen Sie eine Version."
        }
        "Document changed in Rust" => "Das Dokument wurde durch den Rust-Dienst geändert",
        "Enter a Space name." => "Geben Sie einen Namen für den Arbeitsbereich ein.",
        "Open a folder before saving a Space." => {
            "Öffnen Sie einen Ordner, bevor Sie einen Arbeitsbereich speichern."
        }
        "This Space no longer exists." => "Dieser Arbeitsbereich existiert nicht mehr.",
        "The Space folder is unavailable." => "Der Ordner des Arbeitsbereichs ist nicht verfügbar.",
        "This Space is already open in another window." => {
            "Dieser Arbeitsbereich ist bereits in einem anderen Fenster geöffnet."
        }
        "Close this Space in its other window before deleting it." => {
            "Schließen Sie den Arbeitsbereich im anderen Fenster, bevor Sie ihn löschen."
        }
        "Space removed" => "Arbeitsbereich entfernt",
        "The active document changed. Reopen Export to try again." => {
            "Das aktive Dokument wurde gewechselt. Öffnen Sie den Export erneut."
        }
        "Select a supported image file." => "Wählen Sie eine unterstützte Bilddatei aus.",
        "The document has no parent directory." => "Das Dokument hat keinen übergeordneten Ordner.",
        "The assets path must be a directory." => "Der Pfad für Bilder muss ein Ordner sein.",
        "Mermaid diagram · native source view" => "Mermaid-Diagramm · Quelltextansicht",
        "page break" => "Seitenumbruch",
        "Video" => "Video",
        "opens externally" => "wird extern geöffnet",
        "NOTE" => "HINWEIS",
        "TIP" => "TIPP",
        "IMPORTANT" => "WICHTIG",
        "WARNING" => "WARNUNG",
        "CAUTION" => "ACHTUNG",
        "words" => "Wörter",
        "characters" => "Zeichen",
        "lines" => "Zeilen",
        "Open a folder to browse your Markdown files." => {
            "Öffnen Sie einen Ordner, um Ihre Markdown-Dateien zu durchsuchen."
        }
        "Use Split view to write beside the preview." => {
            "In der geteilten Ansicht können Sie neben der Vorschau schreiben."
        }
        "Use File > Spaces to save a folder and its open documents." => {
            "Unter Datei > Arbeitsbereiche speichern Sie einen Ordner mit seinen geöffneten Dokumenten."
        }
        "Use Insert for tables, code blocks, math, and diagrams." => {
            "Über Einfügen fügen Sie Tabellen, Codeblöcke, Formeln und Diagramme hinzu."
        }
        "# Heading" => "# Überschrift",
        "**bold**  *italic*  ~~strikethrough~~" => "**fett**  *kursiv*  ~~durchgestrichen~~",
        "- List item" => "- Listeneintrag",
        "- [ ] Task" => "- [ ] Aufgabe",
        "[Link](destination)" => "[Link](ziel)",
        "> Quotation" => "> Zitat",
        "Ctrl/Cmd+Z: Undo" => "Strg/Cmd+Z: Rückgängig",
        "Ctrl+Y or Ctrl/Cmd+Shift+Z: Redo" => "Strg+Y oder Strg/Cmd+Umschalt+Z: Wiederholen",
        "Ctrl/Cmd+F: Find in Document" => "Strg/Cmd+F: Im Dokument suchen",
        "Ctrl/Cmd+Shift+F: Search in Folder" => "Strg/Cmd+Umschalt+F: Im Ordner suchen",
        "Ctrl/Cmd+S: Save" => "Strg/Cmd+S: Speichern",
        "Ctrl/Cmd+W: Close Document" => "Strg/Cmd+W: Dokument schließen",
        "Opened " => "Geöffnet: ",
        "Space saved: " => "Arbeitsbereich gespeichert: ",
        "Opened Space: " => "Arbeitsbereich geöffnet: ",
        "Exported " => "Exportiert: ",
        "Export failed: " => "Export fehlgeschlagen: ",
        "Could not start MCP: " => "MCP konnte nicht gestartet werden: ",
        "Could not rotate the MCP token: " => "MCP-Token konnte nicht erneuert werden: ",
        "Linked note not found: " => "Verknüpfte Notiz nicht gefunden: ",
        "External link: " => "Externer Link: ",
        "Anchor: " => "Anker: ",
        "Recover " => "Wiederherstellen: ",
        "Restore " => "Wiederherstellen: ",
        "Open " => "Öffnen: ",
        "Remove " => "Entfernen: ",
        "Workspace changed: " => "Arbeitsbereich geändert: ",
        "Enter a line number from 1 to " => "Geben Sie eine Zeilennummer von 1 bis ",
        "CalmMD — a local Markdown editor\n\nYour documents stay on your computer." => {
            "CalmMD – ein lokaler Markdown-Editor\n\nIhre Dokumente bleiben auf Ihrem Computer."
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn german_catalog_covers_the_document_and_tab_controls() {
        assert_eq!(super::german("Preview"), "Vorschau");
        assert_eq!(super::german("Open in New Tab"), "In neuem Tab öffnen");
        assert_eq!(super::german("Interface Language"), "Oberflächensprache");
        assert_eq!(super::german("letter.md"), "letter.md");
    }
}
