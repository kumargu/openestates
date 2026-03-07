"""
OpenEstates Prototype - Entry Point
Run with: python app/main.py
"""
import sys
import os

# Allow imports from project root
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.tui import OpenEstatesApp


def main():
    app = OpenEstatesApp()
    app.run()


if __name__ == "__main__":
    main()