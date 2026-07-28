package com.superidm.di

import android.content.Context
import androidx.work.WorkManager
import com.superidm.data.db.AppDatabase
import com.superidm.data.db.PerSiteRuleDao
import com.superidm.protocol.TorrentEngine
import com.superidm.scheduler.BandwidthLimiter
import com.superidm.scheduler.ScheduleDao
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object Phase2Module {
    
    @Provides
    @Singleton
    fun provideTorrentEngine(@ApplicationContext context: Context): TorrentEngine {
        return TorrentEngine(context)
    }
    
    @Provides
    @Singleton
    fun provideBandwidthLimiter(): BandwidthLimiter {
        return BandwidthLimiter()
    }
    
    @Provides
    fun provideWorkManager(@ApplicationContext context: Context): WorkManager {
        return WorkManager.getInstance(context)
    }
    
    @Provides
    @Singleton
    fun providePerSiteRuleDao(db: AppDatabase): PerSiteRuleDao {
        return db.perSiteRuleDao()
    }
    
    @Provides
    @Singleton
    fun provideScheduleDao(db: AppDatabase): ScheduleDao {
        return db.scheduleDao()
    }
}
