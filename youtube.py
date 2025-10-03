import tkinter as tk

def start(root):
    # Clear existing UI
    for widget in root.winfo_children():
        widget.destroy()

    # Frame for YouTube
    frame = tk.Frame(root, bg="white")
    frame.pack(fill="both", expand=True)

    # Logo (placeholder text for now)
    logo = tk.Label(frame, text="YouTube", font=("Arial", 24, "bold"), fg="red", bg="white")
    logo.pack(pady=10)

    # Search bar
    search_var = tk.StringVar()
    search_entry = tk.Entry(frame, textvariable=search_var, font=("Arial", 14), width=30)
    search_entry.pack(pady=5)

    # Search button
    def do_search():
        query = search_var.get()
        print("Searching YouTube for:", query)  # placeholder

    search_btn = tk.Button(frame, text="Search", command=do_search)
    search_btn.pack(pady=5)

    # Back button to return to gui
    from gui import open_youtube  # careful with circular imports
    back_btn = tk.Button(frame, text="Back", command=lambda: root.quit())  # placeholder
    back_btn.pack(pady=20)