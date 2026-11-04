#!/usr/bin/env python3
"""Render reviewable timer overrides. Never install, enable or activate a runtime."""
import argparse,json,pathlib,subprocess

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--schedule',type=pathlib.Path,required=True)
    parser.add_argument('--config',type=pathlib.Path,required=True)
    parser.add_argument('--out',type=pathlib.Path,required=True)
    args=parser.parse_args()
    schedule=json.loads(args.schedule.read_text());config=json.loads(args.config.read_text())
    if (schedule['config_version']!=1 or config.get('live_enabled') is not False
        or config['broker']['environment']!='demo' or config.get('base_currency')!='EUR'
        or config.get('execution_stage') not in ('SHADOW','DEMO')):
        raise ValueError('Only the EUR demo environment is supported')
    exchanges={entry['exchange_mic'] for entry in config['session_deadlines']}
    if not exchanges or exchanges!=set(schedule['approved_exchanges']):
        raise ValueError('Schedule exchange approval differs from runtime authority')
    calendars=[schedule['daily_calendar'],schedule['weekly_calendar']]
    for calendar in calendars:
        if not isinstance(calendar,str) or len(calendar)>160 or any(c in calendar for c in '\n\r\t'):
            raise ValueError('Invalid timer expression')
        subprocess.run(['systemd-analyze','calendar',calendar],check=True,stdout=subprocess.DEVNULL)
    args.out.mkdir(mode=0o700)
    for unit,calendar in zip(['trady-daily.timer','trady-eur-weekly-report.timer'],calendars):
        directory=args.out/(unit+'.d');directory.mkdir(mode=0o700)
        with (directory/'calendar.conf').open('x')as output:
            output.write('[Timer]\nOnCalendar=\nOnCalendar='+calendar+'\n')
    print(json.dumps({'status':'DISABLED_TIMER_OVERRIDES_PREPARED','output':str(args.out),'exchanges':sorted(exchanges),'services_enabled':False}))

if __name__=='__main__':main()
