import click
from . import __version__


@click.group()
@click.version_option(version=__version__)
def main():
    """openestates CLI tool."""
    pass


@main.command()
@click.argument("name")
def hello(name):
    """Greet NAME."""
    click.echo(f"Hello, {name}!")
